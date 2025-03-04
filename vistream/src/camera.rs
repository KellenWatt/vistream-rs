#![allow(dead_code)]
use std::os::unix::net::{SocketAddr, UnixStream};
use std::os::linux::net::{SocketAddrExt};

use std::sync::{RwLock, Arc};
use std::sync::atomic::{AtomicUsize, AtomicBool, Ordering};

use std::process::{Command};
use std::thread::JoinHandle;

use std::marker::PhantomData;

use std::io::{Write};
// use std::borrow::{Cow};

use rmp_serde::decode::Deserializer;
use serde::{Deserialize};

use vistream_protocol::camera::{self, ClientMessage, Status};
use vistream_protocol::stream::LocationData;
// use vistream_protocol::network::{BufferedStream};
// use vistream_protocol::fs::*;

use crate::frame::{self, Frame};

use crate::error::{Error, Result};

// TODO: figure out what is actually useful here. 
// Open questions: 
// - Should this support MJPG frames, or should that be treated as a separate thing?
// + Yes. Frame supports arbitrary data, and has an type alias (DataFrame) for this purpose.
// - do I store it as a nested Vec (or equavlent), or just a u8 array and provide 
//   indexing types? (leaning toward latter)
// + Definitely did the latter
// - If doing the latter, can this be used as the deserialization type for serde?
//      * probably shouldn't just for the sake of protocol integrity.
// + Will not be the deserialize type
//
// Update: 
// - Should Frame be Clone? 
// - Should Fraem be thread safe inherently?
// - Should it implement a form of Rc on the underlying data to prevent copying?
// - Is this handled by FrameView?
// - Should Box<[u8]> be Rc<[u8]>
//      * this will require something more like Rc<RefCell<[u8]>> or somesuch
// - Is this a practical concern?
//

pub trait FrameSource<F: frame::PixelFormat> {
    fn get_frame(&mut self) -> Result<Option<Arc<Frame<F>>>>;
    fn start(&mut self) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
    fn last_frame_id(&self) -> usize;
}

#[derive(Default, Clone)]
pub struct CameraConfig {
    buffer_count: Option<u32>,
    width: Option<u32>,
    height: Option<u32>,
    resize: bool,
    server_exe: Option<String>,
    conn_timeout: Option<std::time::Duration>,
}

impl CameraConfig {
    pub fn new() -> CameraConfig {
        CameraConfig::default()
    }

    pub fn buffer_count(&mut self, count: u32) -> &mut Self {
        self.buffer_count = (count != 0).then_some(count);
        self
    }

    pub fn width(&mut self, width: u32) -> &mut Self {
        self.width = (width != 0).then_some(width);
        self
    }

    pub fn height(&mut self, height: u32) -> &mut Self {
        self.height = (height != 0).then_some(height);
        self
    }

    pub fn resize(&mut self, resize_if_wrong: bool) -> &mut Self {
        self.resize = resize_if_wrong;
        self
    }

    pub fn server_exe(&mut self, exe_name: &str) -> &mut Self {
        self.server_exe = Some(exe_name.to_owned());
        self
    }

    pub fn conn_timeout(&mut self, timeout: std::time::Duration) -> &mut Self {
        self.conn_timeout = Some(timeout);
        self
    }
}

pub enum Worker {
    Worker {
        worker: Option<JoinHandle<Result<()>>>,
        kill_flag: Arc<AtomicBool>,
    },
    Done,
}

impl Worker {
    pub fn spawn<F: FnOnce(Arc<AtomicBool>) -> Result<()> + Send + 'static>(work: F) -> Worker {
        let kill_flag = Arc::new(AtomicBool::new(false));
        let worker_kill_flag = kill_flag.clone();
        let worker = std::thread::spawn(move || {
            work(worker_kill_flag)
        });
        Worker::Worker {
            worker: Some(worker),
            kill_flag,
        }
    }

    pub fn is_finished(&self) -> bool {
        match self {
            Worker::Worker{ref worker, ..} => worker.as_ref().map(|w| w.is_finished()).unwrap(),
            Worker::Done => true,
        }
    }

    pub fn is_joinable(&self) -> bool {
        match self {
            Worker::Worker{..} => true,
            Worker::Done => false,
        }
    }

    pub fn kill(&self) {
        if let Worker::Worker{kill_flag, ..} = self {
            kill_flag.store(true, Ordering::Relaxed);
        }
    }

    pub fn join(&mut self) -> Option<Error>{
        if let Worker::Worker{worker, kill_flag} = self {
            kill_flag.store(true, Ordering::Relaxed);
            if worker.is_none() {
                return None;
            }
            let worker = worker.take().unwrap();
            match worker.join() {
                Ok(res) => { match res {
                    Ok(_) => {
                        return None;
                    }
                    Err(e) => {
                        *self = Worker::Done;
                        return Some(e);
                    }
                }},
                Err(_) => {
                    return None;
                }
            }
        }
        None
    }
}

pub struct Camera<F: frame::PixelFormat + 'static> {
    // unambiguous camera name (most likely the id) 
    name: String,
    control: UnixStream,
    frame_worker: Worker,
    // type explanation (for my future self)
    // Arc -  allows sharing between threads (the reader from the server),
    // RwLock - makes sure the image isn't overwritten while being read,
    // Option - prevents allocation of garbage data before first data
    // Rc - prevents copying at the get_frame level
    // TODO: Can this be zero copy?
    last_frame: Arc<RwLock<Option<Arc<Frame<F>>>>>,
    last_frame_id: Arc<AtomicUsize>,
    enabled: bool, // AtomicBool?
    width: usize,
    height: usize,
}

impl<F: frame::PixelFormat> Camera<F> {
    pub fn new(name: &str, cfg: CameraConfig) -> Result<Camera<F>> {
        let server_exe = cfg.server_exe.unwrap_or("vistream-camera-server".into());
        let resolved_name_cmd = Command::new(&server_exe).args(["resolve", name]).output()?;

        if !resolved_name_cmd.status.success() {
            return Err(Error::Server(String::from_utf8_lossy(&resolved_name_cmd.stderr).to_string()));
        }

        let true_name = std::str::from_utf8(&resolved_name_cmd.stdout).map_err(|_| {
            Error::Server(String::from_utf8_lossy(&resolved_name_cmd.stderr).to_string())
        })?.trim();

        let res = Command::new(&server_exe)
            .arg("check")
            .arg(&true_name)
            .arg("--quiet")
            .output()?;
   
        let cam_proc = if res.status.success() {
            let mut cmd = Command::new(&server_exe);
            cmd.arg("launch");
            cmd.arg("--format");
            cmd.arg(std::str::from_utf8(&F::proto_format().fourcc()).unwrap());
            if let Some(count) = cfg.buffer_count {
                cmd.arg("--buffer_count");
                cmd.arg(count.to_string());
            }
            if let Some(width) = cfg.width {
                cmd.arg("--width");
                cmd.arg(width.to_string());
            }
            if let Some(height) = cfg.height {
                cmd.arg("--height");
                cmd.arg(height.to_string());
            }
            cmd.arg(&true_name);
            Some(cmd.spawn()?)
            // Should really do something here to check for success, since there's always a
            // possible race condition. For now, just assume it works.
        } else { None };
        // println!("camera launched");
        
        std::thread::sleep(std::time::Duration::from_millis(10));
        let addr = SocketAddr::from_abstract_name(&true_name)?;
        let mut source = None;
        let start = std::time::Instant::now();
        while source.is_none() {
            let conn = UnixStream::connect_addr(&addr);
            match conn {
                Ok(conn) => {
                    source = Some(conn)
                }
                Err(_e) => {
                    // println!("{}", e);
                }
            }
            // if let Ok(conn) = BufferedStream::<UnixStream>::connect(&addr) {
            //     source = Some(conn)
            // }
            if let Some(ref timeout) = cfg.conn_timeout {
                if &start.elapsed() >= timeout {
                    return Err(Error::Timeout);
                }
            }
        }
        // let mut source = UnixStream::connect_addr(&addr)?;
        let mut source = source.unwrap();
        // println!("connection established");

        //  TODO make the Resizer
        // - image size - set up resizer if necessary

        source.write(&[ClientMessage::Status.id()])?;
        source.flush()?;
        let status = Status::deserialize(&mut Deserializer::new(&mut source))?;

        println!("{:?}", F::proto_format());
        println!("{:?}", status);

        // if request fails or has wrong format, die
        // if camera process is a child upon death, kill and reap it before death
        if status.format != F::proto_format() {
            if let Some(mut proc) = cam_proc {
                proc.kill().expect(&format!("camera process couldn't be killed ({})", proc.id()));
                proc.wait().expect(&format!("camera process couldn't be waited ({})", proc.id()));
            }
            return Err(Error::IncompatibleFormat);
        }

        // At this point, we have a camera process, and we're able to talk with it.
        // Note, these sizes are not necessarily the same as the the ones requested.
        // Cropping can happen at a later step
        let width = status.width;
        let height = status.height;

        let control = source.try_clone()?;

        let last_frame = Arc::new(RwLock::new(None));
        let last_frame_id = Arc::new(AtomicUsize::new(0));

        let worker_frame = last_frame.clone();
        let worker_frame_id = last_frame_id.clone();
        let frame_worker = Worker::spawn(move |kill_flag: Arc<AtomicBool>| {
            let mut deserializer = Deserializer::new(&mut source);

            while !kill_flag.load(Ordering::Acquire) {
                let frame_msg = camera::Frame::deserialize(&mut deserializer)?;
                if frame_msg.width as usize != width || frame_msg.height as usize != height {
                    // shouldn't ever happen, but you never know.
                    return Err(Error::FrameData);
                }
                let data = frame_msg.data;
                *worker_frame.write().unwrap() = Some(Arc::new(Frame::new(data, width, height)));
                worker_frame_id.fetch_add(1, Ordering::AcqRel);
            }
            // just to be safe
            // #[allow(unreachable_code)]
            Err(Error::Unknown)
        });

        Ok(Camera {
            name: true_name.to_string(),
            control,
            frame_worker,
            last_frame: last_frame,
            last_frame_id: last_frame_id,
            enabled: false,
            width,
            height,
        })
    }
}


impl<F: frame::PixelFormat> FrameSource<F> for Camera<F> {
    fn get_frame(&mut self) -> Result<Option<Arc<Frame<F>>>> {
        // If the frame_worker has stopped for any reason, return nothing,
        // except for the first time, where it returns the error causing it.
        if self.frame_worker.is_finished() {
            if self.frame_worker.is_joinable() {
                // ignore the error, since something has already gone wrong
                let _ = self.control.write(&[ClientMessage::Disconnect.id()]);
                self.control.flush()?;
                return match self.frame_worker.join() {
                    Some(e) => Err(e),
                    None => Ok(None),
                }
            }
            return Ok(None);
        }

        // We know the worker is stll going at this point.

        match self.last_frame.read() {
            Ok(guard) => {
                Ok(guard.clone())
            }
            Err(_) => {
                // The lock has been poisoned, likely because the worker panicked for some reason.
                // Most of the code below makes little to no sense in this context, but is just 
                // checking all the boxes, just in case I'm stupid.
                self.frame_worker.kill();
                let _ = self.control.write(&[ClientMessage::Disconnect.id()]);
                self.control.flush()?;
                match self.frame_worker.join() {
                    Some(e) => Err(e),
                    None => Ok(None),
                }
            }
        }
    }

    fn start(&mut self) -> Result<()> {
        self.control.write(&[ClientMessage::Start.id()])?;
        self.control.flush()?;
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        self.control.write(&[ClientMessage::Stop.id()])?;
        self.control.flush()?;
        Ok(())
    }

    fn last_frame_id(&self) -> usize {
        self.last_frame_id.load(Ordering::Acquire)
    }
}

impl<F: frame::PixelFormat> Drop for Camera<F> {
    fn drop(&mut self) {
        if !self.frame_worker.is_finished() {
            // ignore any write errors. We're in cleanup mode.
            let _ = self.control.write(&[ClientMessage::Disconnect.id()]);
            let _ = self.control.flush(); 
            self.frame_worker.kill();
            self.frame_worker.join();
        } else {
            self.frame_worker.join();
        }
    }
}


pub trait Locate<F: frame::PixelFormat, S: FrameSource<F>> {
    fn locate(&mut self, source: &mut S) -> Result<Vec<LocationData>>;
    fn locate_once(&mut self, source: &mut S) -> Result<Option<LocationData>> {
        Ok(self.locate(source)?.get(0).copied())
    }
    fn contains_target(&mut self, source: &mut S) -> Result<bool> {
        Ok(self.locate(source)?.len() > 0)
    }
}


pub struct FrameSequencer<F: frame::PixelFormat, S: FrameSource<F>> {
    source: S,
    last_seen_id: Option<usize>,
    _format: PhantomData<F>,
}

impl<F: frame::PixelFormat, S: FrameSource<F>> FrameSequencer<F, S> {
    pub fn new(source: S) -> FrameSequencer<F, S> {
        FrameSequencer {
            source,
            last_seen_id: None,
            _format: PhantomData,
        }
    }
}

impl<F: frame::PixelFormat, S: FrameSource<F>> FrameSource<F> for FrameSequencer<F, S> {
    fn get_frame(&mut self) -> Result<Option<Arc<Frame<F>>>> {
        let last_id = self.source.last_frame_id();
        if let Some(id) = self.last_seen_id {
            if id == last_id {
                return Ok(None);
            }
        }
        self.last_seen_id = Some(last_id);
        self.source.get_frame()
    }
    fn start(&mut self) -> Result<()> {
        self.source.start()
    }
    fn stop(&mut self) -> Result<()> {
        self.source.stop()
    }
    fn last_frame_id(&self) -> usize {
        self.source.last_frame_id()
    }
}

use std::time::{Instant, Duration};
pub struct FrameRateLimiter<F: frame::PixelFormat, S: FrameSource<F>> {
    source: S,
    frame_delay: Duration,
    next_frame_time: Instant,
    last_id: usize,
    _format: PhantomData<F>,
}

impl<F: frame::PixelFormat, S: FrameSource<F>> FrameRateLimiter<F, S> {
    pub fn new(source: S, frame_delay: Duration) -> FrameRateLimiter<F, S> {
        let last_id = source.last_frame_id();
        FrameRateLimiter {
            source,
            frame_delay,
            next_frame_time: Instant::now(),
            last_id,
            _format: PhantomData,
        }
    }

    pub fn set_frame_delay(&mut self, frame_delay: Duration) {
        self.next_frame_time -= self.frame_delay;
        self.next_frame_time += frame_delay;
        self.frame_delay = frame_delay;
    }
}

impl<F: frame::PixelFormat, S: FrameSource<F>> FrameSource<F> for FrameRateLimiter<F, S> {
    fn get_frame(&mut self) -> Result<Option<Arc<Frame<F>>>> {
        if Instant::now() < self.next_frame_time {
            return Ok(None);
        }
        self.next_frame_time = Instant::now() + self.frame_delay;
        self.last_id = self.source.last_frame_id();
        self.source.get_frame()
    }

    fn start(&mut self) -> Result<()> {
        self.source.start()
    }
    
    fn stop(&mut self) -> Result<()> {
        self.source.stop()
    }

    fn last_frame_id(&self) -> usize {
        self.last_id
    }
}
