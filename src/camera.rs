#![allow(dead_code)]
// use std::time::Duration;

use libcamera::{
    camera::{self, CameraConfigurationStatus},
    // camera_manager as mgr,
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
use crate::thread::WorkerThread;

// use std::sync::mpsc;
use std::sync::{RwLock, Mutex, Arc};
use std::sync::atomic::{AtomicU64, Ordering};


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

struct Cam(camera::ActiveCamera<'static>);

impl std::ops::Deref for Cam {
    type Target = camera::ActiveCamera<'static>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Cam {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

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
unsafe impl Send for Cam {}


pub struct Camera {
    _camref: &'static camera::Camera<'static>,
    cam: Arc<Mutex<Cam>>,
    cfgs: camera::CameraConfiguration,
    // reqs: Vec<Request>,


    frame_size: (usize, usize),
    frame_id: Arc<AtomicU64>,
    // frame_tx: mpsc::Sender<Request>,
    // frame_rx: mpsc::Reciever<Request>,
    last_frame: Arc<RwLock<Frame>>,
    format: PixelFormat,

    worker: Option<WorkerThread<Request>>,
}

impl Camera {

    fn alloc_requests(&mut self) -> Vec<Request> {
        let mut alloc = FrameBufferAllocator::new(&self.cam.lock().unwrap());

        let cfg = self.cfgs.get(0).unwrap();
        let stream = cfg.stream().unwrap();
        let buffers = alloc.alloc(&stream).unwrap();

        buffers.into_iter().map(|buf| {
            let buf = MemoryMappedFrameBuffer::new(buf).unwrap();
            let mut req = self.cam.lock().unwrap().create_request(None).unwrap();
            req.add_buffer(&stream, buf).unwrap();
            req
        }).collect::<Vec<_>>()
    }

    fn make_worker(&mut self) -> WorkerThread<Request> {
        let thread_cam = self.cam.clone();
        let thread_last_frame = self.last_frame.clone();
        let thread_frame_id = self.frame_id.clone();
        let cfg = self.cfgs.get(0).unwrap();
        
        let stream = cfg.stream().unwrap();
        let width = cfg.get_size().width as usize;
        let height = cfg.get_size().height as usize;

        let worker = WorkerThread::new(move |rx| {
            let mut req: Request = rx.recv()?;
            let frame_buf: &MemoryMappedFrameBuffer<FrameBuffer> = req.buffer(&stream).unwrap();
            let planes = frame_buf.data();
            let frame_data = planes.get(0).unwrap();

            let frame_len = frame_buf.metadata().unwrap().planes().get(0).unwrap().bytes_used as usize;

            let frame = Frame::new(width, height, &frame_data[..frame_len]);
            
            *thread_last_frame.write().unwrap() = frame;
            thread_frame_id.fetch_add(1, Ordering::AcqRel);

            req.reuse(ReuseFlag::REUSE_BUFFERS);
            thread_cam.lock().unwrap().queue_request(req)?;
            Ok(())
        });
        worker
    }

    fn from_config(srcam: &'static camera::Camera<'static>,
                   format: PixelFormat,
                   // mut cam: camera::ActiveCamera<'static>,
                   mut cfgs: camera::CameraConfiguration) -> Option<Camera> {
        let mut cam = srcam.acquire().ok()?;
        match cfgs.validate() {
            CameraConfigurationStatus::Valid => Some(()),
            CameraConfigurationStatus::Adjusted => {
                if cfgs.get(0).unwrap().get_pixel_format() != format.libcamera_pixel_format() {
                    None
                } else {
                    Some(())
                }
            }
            CameraConfigurationStatus::Invalid => None,
        }?;

        cam.configure(&mut cfgs).ok()?;
        let cfg = cfgs.get(0).unwrap();
        // let stream = cfg.stream().unwrap();

        // let (tx, rx) = mpsc::channel();
 // 
        // cam.on_request_completed(move |req| {
        //     tx.send(req).unwrap();
        // });

        let width = cfg.get_size().width as usize;
        let height = cfg.get_size().height as usize;

        let last_frame = Arc::new(RwLock::new(Frame::blank(width, height)));

        let cam = Cam(cam);
        let cam = Arc::new(Mutex::new(cam));

        // let thread_cam = cam.clone();
        // let thread_last_frame = last_frame.clone();
        // 
        let frame_id = Arc::new(AtomicU64::new(0));
        // let thread_frame_id = frame_id.clone();
        // 
        // let worker = SignaledThread::new(move || {
        //     let mut req = rx.recv()?;
        //     let frame_buf: &MemoryMappedFrameBuffer<FrameBuffer> = req.buffer(&stream).unwrap();
        //     let planes = frame_buf.data();
        //     let frame_data = planes.get(0).unwrap();
        // 
        //     let frame_len = frame_buf.metadata().unwrap().planes().get(0).unwrap().bytes_used as usize;
        // 
        //     let frame = Frame::new(width, height, &frame_data[..frame_len]);
        //     
        //     *thread_last_frame.write().unwrap() = frame;
        //     thread_frame_id.fetch_add(1, Ordering::AcqRel);
        // 
        //     req.reuse(ReuseFlag::REUSE_BUFFERS);
        //     thread_cam.lock().unwrap().queue_request(req)?;
        //     Ok(())
        // });

        Some(Camera {
            _camref: srcam,
            cam,
            cfgs,
            frame_size: (width, height),
            frame_id,
            last_frame,
            format,
            worker: None
        })
    }

    pub fn new(cam: &'static camera::Camera<'static>, format: PixelFormat) -> Option<Camera> {
        let mut cfgs = cam.generate_configuration(&[StreamRole::VideoRecording]).unwrap();
        let mut cfg = cfgs.get_mut(0).unwrap();
        cfg.set_pixel_format(format.libcamera_pixel_format());
        cfg.set_buffer_count(4);
        Camera::from_config(cam, format, cfgs)
    }

    pub fn with_size(cam: &'static camera::Camera<'static>, 
                     width: usize, height: usize,
                     format: PixelFormat) -> Option<Camera> {
        let mut cfgs = cam.generate_configuration(&[StreamRole::VideoRecording]).unwrap();
        let mut cfg = cfgs.get_mut(0).unwrap();
        cfg.set_pixel_format(format.libcamera_pixel_format());
        cfg.set_buffer_count(4);
        cfg.set_size(Size{width: width as u32, height: height as u32});
        Camera::from_config(cam, format, cfgs)
    }
}

impl FrameSource for Camera {
    fn get_frame(&self) -> Frame {
        self.last_frame.read().unwrap().clone()
    }

    fn start(&mut self) -> Result<(), anyhow::Error> {
        if self.worker.is_some() {
            // FIXME
            // TODO
            // FIXME
            panic!("Figure this out later") 
        }
        self.cam.lock().unwrap().start(None).unwrap();
       
        let reqs = self.alloc_requests(); 
        let worker = self.make_worker();
        self.worker = Some(worker);
        
        let tx = self.worker.as_ref().unwrap().sender();
        self.worker.as_mut().unwrap().start()?;
      
        self.cam.lock().unwrap().on_request_completed(move |req| {
            tx.send(req).unwrap();
        });

        for req in reqs {
            self.cam.lock().unwrap().queue_request(req).unwrap();
        }
        Ok(())
    }

    fn stop(&mut self) -> Result<(), anyhow::Error> {
        if self.worker.is_none() {
            // FIXME
            // TODO
            // FIXME
            panic!("Figure this out later") 
        }
        let mut worker = self.worker.take().unwrap();
        worker.kill();
        worker.join()?;

        self.cam.lock().unwrap().stop()?;
        Ok(())
    }

    fn frame_size(&self) -> (usize, usize) {
        self.frame_size
    }

    fn frame_id(&self) -> u64 {
        self.frame_id.load(Ordering::Acquire)
    }
}

pub unsafe fn enable_libcamera_logging(enable: bool) {
    if enable {
        std::env::remove_var("LIBCAMERA_LOG_LEVELS");
    } else {
        std::env::set_var("LIBCAMERA_LOG_LEVELS", "*:4");
    }
}

