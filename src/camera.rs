#![allow(dead_code)]
// use std::time::Duration;

use libcamera::{
    camera::{self, CameraConfigurationStatus},
    camera_manager as mgr,
    framebuffer::AsFrameBuffer,
    framebuffer_allocator::{FrameBuffer, FrameBufferAllocator},
    framebuffer_map::MemoryMappedFrameBuffer,
    pixel_format as pf,
    // properties, 
    stream::StreamRole,
    request::{ReuseFlag, Request},
    geometry::Size,
};

// use crate::signal_thread::SignaledThread;
// use crate::thread::WorkerThread;

use std::sync::mpsc;
use std::sync::{RwLock, Mutex, Arc};
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

use std::rc::Rc;

pub enum PixelFormat {
    RGB,
    BGR,
    RGBA,
    BGRA,
    YUYV,
    MJPEG,
}

impl PixelFormat {
    /// Returns the FourCC representation used for this pixel format. This information is
    /// largely presented to be informational, but if there is a use for it, it's here.
    ///
    /// As a note, the representations for any RGB/BGR are backwards because of how
    /// libcamera and likely the underlying Linux kernel handle byte order. If your R and B
    /// are swapped, switch to other option, and that should fix your issue.
    pub fn fourcc(&self) -> [u8; 4] {
        match self {
            PixelFormat::RGB => *b"BG24",
            PixelFormat::BGR => *b"RG24",
            PixelFormat::RGBA => *b"RA24",
            PixelFormat::BGRA => *b"BA24",
            PixelFormat::YUYV => *b"YUYV",
            PixelFormat::MJPEG => *b"MJPG"
        }
    }

    fn libcamera_pixel_format(&self) -> pf::PixelFormat {
        pf::PixelFormat::new(u32::from_le_bytes(self.fourcc()), 0)
    }
}


pub trait FrameSource {
    fn get_frame(&self) -> Frame;
    fn frame_size(&self) -> (usize, usize);
    fn frame_id(&self) -> u64;
    fn start(&mut self) -> Result<(), anyhow::Error>;
    fn stop(&mut self) -> Result<(), anyhow::Error>;
    // fn field_of_view(&self) -> (f32, f32);
}

#[derive(Clone)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub data: Box<[u8]>,
}

impl Frame {
    pub fn new(width: usize, height: usize, data: &[u8]) -> Frame {
        Frame {
            width,
            height,
            data: data.to_vec().into_boxed_slice(),
        }
    }

    fn blank(width: usize, height: usize) -> Frame {
        Frame {
            width,
            height,
            data: vec![0; width * height].into_boxed_slice(),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.data.len() == 0
    }
}

struct Cam<'a>(camera::Camera<'a>);

// We have to do this because the cam is functionally owned by the thread, but 
// we're keeping it around basically just for start and stop

// After checking the source of both libcamera-rs and libcamera, this is probably fine
// The most dangerous data is the thread handle of the callback, and the likely access
// to the file system needed to access the cameras. Neither of those should be affected
// here. File systems tend to be inherently locked, and the handle shouldn't be doing any 
// work in the worker thread. Also any and all requests will be owned and
// handled by the worker thread, once sent. At the very least there won't be cross-contamination 
// between the threads, once started. The final compiler pinch point is that libcamera_request_t
// isn't Send, except Request is, so it should be completely fine. Also, the two threads should
// never conflict on requests, since buffers won't be user-specified.
unsafe impl Send for Cam<'_> {}

struct Cfg(camera::CameraConfiguration);
unsafe impl Send for Cfg {}

struct Mgr(mgr::CameraManager);
unsafe impl Send for Mgr {}

// This is not true in general! It's only true in this case where a great deal of care has 
// gone into making sure its safe enough. This is purely to "trick" the compiler into accepting
// safe-ish code, and is very much not the case in any other code other than when we're using 
// a mutex to lock the Mgr down during writes.
unsafe impl Sync for Mgr {}

static mut mgr_instanced: AtomicBool = AtomicBool::new(false);

pub struct CameraManager {
    // This absolute abomination of a type exists so that a reference to the manager can 
    // exist within each thread, but it's still safe to access the manager during the
    // initial allocations.
    //
    // To do this, the outer Arc is cloned with the Mutex to share with each thread.
    // Then the Mutex locks down the manager, so it can create the camera list safely and
    // allocate the camera. Specifically, all of the allocations are made using a clone
    // of the inner Arc, which is still technically locked down because of the Mutex. Once 
    // this is done, the lock is lifted, and the inner clone is left behind as a read-only 
    // reference for the Camera and all associated types.
    mgr: Arc<Mutex<Arc<Mgr>>>,
}

impl CameraManager {
    pub fn new() -> Option<CameraManager> {
        if unsafe {mgr_instanced.load(Ordering::Acquire)} {
            return None;
        }

        let res = Some(CameraManager {
            mgr: Arc::new(Mutex::new(Arc::new(Mgr(mgr::CameraManager::new().ok()?))))
        });
        unsafe {
            mgr_instanced.store(true, Ordering::Release);
        }

        res
    }

    pub fn count(&self) -> usize {
        self.mgr.lock().unwrap().0.cameras().len()
    }

    fn instance(&self) -> Arc<Mutex<Arc<Mgr>>> {
        self.mgr.clone()
    }

}

impl Drop for CameraManager {
    fn drop(&mut self) {
        unsafe {
            mgr_instanced.store(false, Ordering::Release);
        }
    }
}


enum Control {
    Start,
    Stop,
    Shutdown,
}

pub struct Camera {
    last_frame: Arc<RwLock<Frame>>,
    frame_id: Arc<AtomicU64>,
    worker: JoinHandle<()>,
    ctl: mpsc::Sender<Control>,
    size: (usize, usize),
}

impl Camera {
    pub fn new(cam_index: usize, mgr: &CameraManager, format: PixelFormat) -> Option<Camera> {
        let _mgr = mgr.mgr.lock().unwrap();
        let _cameras = _mgr.0.cameras();
        let cam = _cameras.get(cam_index).unwrap();
        println!("  getting cam");
        let mut cfgs = cam.generate_configuration(&[StreamRole::VideoRecording]).unwrap();
        let mut cfg = cfgs.get_mut(0).unwrap();
        cfg.set_pixel_format(format.libcamera_pixel_format());
        cfg.set_buffer_count(4);
        println!("  set config");
        Camera::from_config(mgr, cam_index, format, cfgs)
    }

    pub fn with_size(cam_index:usize, mgr: &CameraManager, 
                     width: usize, height: usize,
                     format: PixelFormat) -> Option<Camera> {
        let _mgr = mgr.mgr.lock().unwrap();
        let _cameras = _mgr.0.cameras();
        let cam = _cameras.get(cam_index).unwrap();
        let mut cfgs = cam.generate_configuration(&[StreamRole::VideoRecording]).unwrap();
        let mut cfg = cfgs.get_mut(0).unwrap();
        cfg.set_pixel_format(format.libcamera_pixel_format());
        cfg.set_buffer_count(4);
        cfg.set_size(Size{width: width as u32, height: height as u32});
        Camera::from_config(mgr, cam_index, format, cfgs)
    }
    fn from_config(mgr: &CameraManager, cam_index: usize, 
                   format:PixelFormat, 
                   mut cfgs: camera::CameraConfiguration) -> Option<Camera> {

        if cam_index >= mgr.count() {
            return None;
        }
        let cfgs = Cfg(cfgs);

        println!("  getting mgr copy");
        let mgr = mgr.instance();

        let cfg = cfgs.0.get(0).unwrap();
        let size = cfg.get_size();
        println!("  setting up thread vars");
        let (ctl_tx, ctl_rx) = mpsc::channel();
        let last_frame = Arc::new(RwLock::new(Frame::blank(size.width as usize, size.height as usize)));
        let frame_id = Arc::new(AtomicU64::new(0));

        let thread_last_frame = last_frame.clone();
        let thread_frame_id = frame_id.clone();


        let thread_builder = thread::Builder::new();
        let worker = thread_builder.spawn(move || {
            let mut cfgs = cfgs;
            let mgr_lock = mgr.lock().unwrap();
            let mgr = mgr_lock.clone();
            let cameras = mgr.0.cameras();
            let srccam = cameras.get(cam_index).unwrap();

            match cfgs.0.validate() {
                CameraConfigurationStatus::Valid => Some(()),
                CameraConfigurationStatus::Adjusted => {
                    if cfgs.0.get(0).unwrap().get_pixel_format() != format.libcamera_pixel_format() {
                        None
                    } else {
                        Some(())
                    }
                }
                CameraConfigurationStatus::Invalid => None,
            }.unwrap();
            // let srccam = Cam(srccam);
            
            let mut cam = srccam.acquire().unwrap();
            cam.configure(&mut cfgs.0).unwrap();
            let cfg = cfgs.0.get(0).unwrap();
            let stream = cfg.stream().unwrap();
           
            
            drop(mgr_lock);

            let (tx, rx) = mpsc::channel();
            cam.on_request_completed(move |req| {
                tx.send(req).unwrap();
            });
            
            let mut term_flag = false;
            let mut running = false;
            while !term_flag {
                match ctl_rx.try_recv() {
                    Ok(msg) => match msg {
                        Control::Start => {
                            let mut alloc = FrameBufferAllocator::new(&cam);

                            let buffers = alloc.alloc(&stream).unwrap();
                            let reqs = buffers.into_iter().map(|buf| {
                                let buf = MemoryMappedFrameBuffer::new(buf).unwrap();
                                let mut req = cam.create_request(None).unwrap();
                                req.add_buffer(&stream, buf).unwrap();
                                req
                            }).collect::<Vec<_>>();



                            cam.start(None).unwrap();
                            // Submit requests here because there isn't really a good place to do that otherwise.
                            for req in reqs {
                                cam.queue_request(req).unwrap();
                            }
                            running = true;
                        }
                        Control::Stop => {
                            // stop camera stream
                            running = false;
                            cam.stop();
                            while rx.try_recv().is_ok() {
                                // drain the channel
                                // nothing doing here
                            }
                        }
                        Control::Shutdown => {
                            if running {
                                running = false;
                                cam.stop();
                                term_flag = true;
                            }
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => {/* do nothing */}
                    Err(mpsc::TryRecvError::Disconnected) => {
                        // Do not set term_flag, since shutting down the control channel means
                        // this thread is no longer reachable. That is defined, but breaking
                        // behaviour.
                        break;
                    } 
                }
                if !running {
                    thread::sleep(std::time::Duration::from_millis(10));
                    continue;
                }

                // do camera shenanigans
                match rx.try_recv() {
                    Ok(mut req) => {
                        // do processing;
                        let frame_buf: &MemoryMappedFrameBuffer<FrameBuffer> = req.buffer(&stream).unwrap();
                        let planes = frame_buf.data();
                        let frame_data = planes.get(0).unwrap();

                        let frame_len = frame_buf.metadata().unwrap().planes().get(0).unwrap().bytes_used as usize;
                        let frame = Frame::new(size.width as usize, size.height as usize, &frame_data[..frame_len]);
                        *thread_last_frame.write().unwrap() = frame;

                        thread_frame_id.fetch_add(1, Ordering::AcqRel);

                        req.reuse(ReuseFlag::REUSE_BUFFERS);
                        cam.queue_request(req).unwrap();
                    }
                    Err(mpsc::TryRecvError::Empty) => {continue;} // basically no-op
                    Err(mpsc::TryRecvError::Disconnected) => {break;} // something happened
                }
            }
        });

        let worker = worker.ok()?;

        Some(Camera {
            last_frame,
            frame_id,
            worker,
            ctl: ctl_tx,
            size: (size.width as usize, size.height as usize)
        })
    }
}

impl FrameSource for Camera {
    fn get_frame(&self) -> Frame {
        self.last_frame.read().unwrap().clone()
    }
    fn start(&mut self) -> Result<(), anyhow::Error> {
        self.ctl.send(Control::Start)?;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), anyhow::Error> {
        self.ctl.send(Control::Stop)?;
        Ok(())
    }

    fn frame_size(&self) -> (usize, usize) {
        self.size
    }

    fn frame_id(&self) -> u64 {
        self.frame_id.load(Ordering::Acquire)
    }
}
// impl FrameSource for Camera {
//     fn get_frame(&self) -> Frame {
//         self.last_frame.read().unwrap().clone()
//     }
// 
//     fn start(&mut self) -> Result<(), anyhow::Error> {
//         if self.worker.is_some() {
//             // FIXME
//             // TODO
//             // FIXME
//             panic!("Figure this out later") 
//         }
//         self.cam.lock().unwrap().start(None).unwrap();
//        
//         let reqs = self.alloc_requests(); 
//         let worker = self.make_worker();
//         self.worker = Some(worker);
//         
//         let tx = self.worker.as_ref().unwrap().sender();
//         self.worker.as_mut().unwrap().start()?;
//       
//         self.cam.lock().unwrap().on_request_completed(move |req| {
//             tx.send(req).unwrap();
//         });
// 
//         for req in reqs {
//             self.cam.lock().unwrap().queue_request(req).unwrap();
//         }
//         Ok(())
//     }
// 
//     fn stop(&mut self) -> Result<(), anyhow::Error> {
//         if self.worker.is_none() {
//             // FIXME
//             // TODO
//             // FIXME
//             panic!("Figure this out later") 
//         }
//         let mut worker = self.worker.take().unwrap();
//         worker.kill();
//         worker.join()?;
// 
//         self.cam.lock().unwrap().stop()?;
//         Ok(())
//     }
// 
//     fn frame_size(&self) -> (usize, usize) {
//         self.frame_size
//     }
// 
//     fn frame_id(&self) -> u64 {
//         self.frame_id.load(Ordering::Acquire)
//     }
// }

pub unsafe fn enable_libcamera_logging(enable: bool) {
    if enable {
        std::env::remove_var("LIBCAMERA_LOG_LEVELS");
    } else {
        std::env::set_var("LIBCAMERA_LOG_LEVELS", "*:4");
    }
}

