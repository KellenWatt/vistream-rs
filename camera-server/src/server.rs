use crate::parser::{Launch, FourCC};
use vistream_protocol::camera::{PixelFormat, Frame, ClientMessage, Status};
use vistream_protocol::fs::*;
use crate::shared::*;

use serde::{Serialize};
use rmp_serde::{Serializer};

macro_rules! unwrap_or_fail_with_free {
    ($code: expr, $name: expr, $value: expr) => {
        match $value {
            Ok(v) => v,
            Err(e) => {
                let _ = free_camera($name);
                fail!($code, e.to_string());
            }
        }
    }
}

macro_rules! fail_with_free {
    ($code:expr, $name:expr, $e:expr) => {
        let _ = free_camera($name);
        fail!($code, $e)
    };
    ($code:expr, $name:expr, $($tts:tt)*) => {
        let _ = free_camera($name);
        fail!($code, $($tts)*);
    };
}


use libcamera::{
    camera::{
        CameraConfigurationStatus,
        Camera,
    },
    camera_manager::{CameraManager, CameraList},
    framebuffer::AsFrameBuffer,
    framebuffer_allocator::{FrameBuffer, FrameBufferAllocator},
    framebuffer_map::MemoryMappedFrameBuffer,
    pixel_format as pf,
    properties,
    stream::{
        StreamRole,
        StreamConfigurationRef,
    },
    geometry::Size,
    request::ReuseFlag,
};

use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};
use std::os::linux::net::{SocketAddrExt};

use std::sync::mpsc::{self, TryRecvError, RecvTimeoutError};

use std::fs::{File};
use std::io::{Write, Read};


struct CamIter<'a> {
    list: &'a CameraList<'a>,
    index: usize,
}

impl<'a> CamIter<'a> {
    fn new(cameras: &'a CameraList<'a>) -> CamIter<'a> {
        CamIter {
            list: cameras,
            index: 0,
        }
    }
}

impl<'a> Iterator for &mut CamIter<'a> {
    type Item = Camera<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        let res = self.list.get(self.index)?;
        self.index += 1;
        Some(res)
    }
}

struct Connection {
    socket: UnixStream,
    #[allow(dead_code)]
    addr: SocketAddr,
    // serializer: Serializer<UnixStream>,
    healthy: bool,
    active: bool,
}

impl Connection {
    fn new(conn: (UnixStream, SocketAddr)) -> Connection {
        
        conn.0.set_nonblocking(true).unwrap();
        Connection {
            socket: conn.0,
            addr: conn.1,
            // serializer: Serializer::new(conn.0),
            healthy: true,
            active: false,
        }
    }

    fn serializer(&mut self) -> Serializer<&mut UnixStream> {
        Serializer::new(&mut self.socket)
    }

    fn poison(&mut self) {
        self.healthy = false;
        self.active = false;
    }
    fn is_healthy(&self) -> bool {
        self.healthy
    }

    fn activate(&mut self) {
        self.active = self.healthy;
    }
    fn deactivate(&mut self) {
        self.active = false;
    }
    fn is_active(&self) -> bool {
        self.healthy && self.active
    }
}

pub fn launch(data: Launch) -> VisResult<()> {
    // open questions:
    // - buffered? possible smoother framerate, but data is technically less "live"

    // Acquire camera
    // create socket, named after id (using an abstract name)
    // loop {
    //  check for connections (try, then move on)
    //  if any connections {
    //    request next frame
    //
    //    on request complete, send to all connections immediately
    //      if any connections fail, clean them up
    //    recycle request
    //  }
    // }
   
    let mgr = unwrap_or_fail!(4, CameraManager::new());
    let cameras = mgr.cameras();
    let mut camera_list = CamIter::new(&cameras);
    
    let name = resolve_alias(&data.name)?;
    let (cam, full_name) = match get_camera_by_name(&mut camera_list, &name)? {
        Some(res) => res,
        None => {fail!(6, "no camera exists with name '{}'", name);}
    };
    if is_camera_used(&full_name)? {
        // fail silently if server is already started
        if data.allow_fail {
            std::process::exit(0);
        }
        println!("{}", full_name);
        fail!(255, "camera {} already in use", full_name);
    } 
    let mut cam = unwrap_or_fail!(6, cam.acquire());

    use_camera(&full_name)?;
    println!("{}", full_name);
    let name = full_name.clone();
    unwrap_or_fail_with_free!(10, &full_name, ctrlc::set_handler(move || {
        match free_camera(&name) {
            Ok(()) => std::process::exit(0),
            Err(code) => std::process::exit(code as i32),
        };
    }));

    let mut cfgs = cam.generate_configuration(&[StreamRole::VideoRecording]).unwrap();
    let mut cfg = cfgs.get_mut(0).unwrap();

    let format = translate_pixel_format(data.format);
    cfg.set_pixel_format(format);
    match get_closest_size(&cfg, &data) {
        Some(size) => cfg.set_size(size),
        None => {
            fail_with_free!(10, &full_name, "{:?} is unsupported for {}", data.format, full_name);
        },
    };
    cfg.set_buffer_count(data.buffer_count);

    match cfgs.validate() {
        CameraConfigurationStatus::Valid => {/* true no-op */}
        CameraConfigurationStatus::Adjusted => {/* feels like something should be done, but no-op */}
        CameraConfigurationStatus::Invalid => {
            fail_with_free!(8, &full_name, "valid camera config could not be generated");
        },
    };
    unwrap_or_fail_with_free!(8, &full_name, cam.configure(&mut cfgs));

    let cfg = cfgs.get(0).unwrap();
    let size = cfg.get_size();

    // we're working on the assumption that the camera's id is unique.
    // This may not be true globally, but it almost certainly will be in a vast
    // majority of situations.
    let addr = unwrap_or_fail_with_free!(7, &full_name, SocketAddr::from_abstract_name(cam.id()));
    let listener = unwrap_or_fail_with_free!(7, &full_name, UnixListener::bind_addr(&addr));
    unwrap_or_fail_with_free!(7, &full_name, listener.set_nonblocking(true));

    let mut connections: Vec<Connection> = Vec::new();
    
    // Allocate frame buffers for the stream
    let mut alloc = FrameBufferAllocator::new(&cam);
    let stream = cfg.stream().unwrap();
    let buffers = alloc.alloc(&stream).unwrap();

    let reqs = buffers.into_iter().map(|buf| {
        let buf = MemoryMappedFrameBuffer::new(buf).unwrap();
        let mut req = cam.create_request(None).unwrap();
        req.add_buffer(&stream, buf).unwrap();
        req
    }).collect::<Vec<_>>();

    // Completed capture requests are returned as a callback
    let (tx, rx) = mpsc::channel();
    cam.on_request_completed(move |req| {
        tx.send(req).unwrap();
    });

    cam.start(None).unwrap();

    for req in reqs {
        unwrap_or_fail!(8, cam.queue_request(req));
    }

    let mut unused_reqs = Vec::new();
    let pixel_format = data.format.to_string().parse().unwrap();

    loop {
        match listener.accept() {
            Ok(conn) => {
                connections.push(Connection::new(conn));
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    // do nothing, this is the norm. 
                    // possible use for this in the future: "sleeping" the check for 
                    // a fraction of a second, just to save on processing, since
                    // incoming connections should be rare relative to the loop.
                } else {
                    fail_with_free!(7, &full_name, e.to_string());
                }
            }
        }

        for conn in connections.iter_mut() {
            let mut buf = [0u8];
            let action = match conn.socket.read(&mut buf) {
                Ok(n) if n == 1 => ClientMessage::from_id(buf[0]),
                Ok(_) => {continue;}
                Err(_) => {
                    conn.poison();
                    None
                }
            };

            match action {
                Some(ClientMessage::Start) => conn.activate(),
                Some(ClientMessage::Stop) => conn.deactivate(),
                Some(ClientMessage::Disconnect) => conn.poison(),
                Some(ClientMessage::Status) => {
                    // TODO status stuff
                    // - enabled
                    // - encoding
                    // - frame size
                    let msg = Status {
                        enabled: conn.is_active(),
                        healthy: conn.is_healthy(),
                        format: data.format.to_string().parse().unwrap(),
                        width: size.width as usize,
                        height: size.height as usize,
                    };
                    match msg.serialize(&mut conn.serializer()) {
                        Ok(_) => {/* no-op*/ }
                        Err(_) => {
                            // there may be occasions that this isn't grounds for poisoning, 
                            // but I don't know of any
                            conn.poison();
                        }
                    }
                }
                _ => unreachable!()
            }
        }


        if connections.is_empty() || connections.iter().all(|conn| !conn.is_active()){
            match rx.try_recv() {
                Ok(mut req) => {
                    // discard frame data
                    let req = req.reuse(ReuseFlag::REUSE_BUFFERS);
                    unused_reqs.push(req);
                }
                Err(TryRecvError::Empty) => {/* do nothing */}
                Err(TryRecvError::Disconnected) => {fail_with_free!(9, &full_name, "camera disconnected");}
            }
            connections = connections.into_iter().filter(|conn| conn.is_healthy()).collect();
            std::thread::sleep(std::time::Duration::from_millis(20));
            continue;
        }

        let mut req = match rx.recv_timeout(std::time::Duration::from_millis(10)) {
            Ok(req) => req,
            Err(RecvTimeoutError::Timeout) => {continue;}
            Err(RecvTimeoutError::Disconnected) => {fail_with_free!(9, &full_name, "camera disconnected");}
        };
        
        // get frame
        
        let frame_buffer: &MemoryMappedFrameBuffer<FrameBuffer> = req.buffer(&stream).unwrap();
        let planes = frame_buffer.data();
        let frame_data = planes.get(0).unwrap();
        let bytes_used = frame_buffer.metadata().unwrap().planes().get(0).unwrap().bytes_used as usize;

        let frame = Frame {
            format: pixel_format,
            width: size.width,
            height: size.height,
            data: &frame_data[..bytes_used],
        };
        
        for conn in connections.iter_mut() {
            match frame.serialize(&mut conn.serializer()) {
                Ok(_) => {/* no-op*/ }
                Err(_) => {
                    // there may be occasions that this isn't grounds for poisoning, 
                    // but I don't know of any
                    conn.poison();
                }
            };
        }
        // pruning dead connections
        connections = connections.into_iter().filter(|conn| conn.is_healthy()).collect();

        req.reuse(ReuseFlag::REUSE_BUFFERS);
        unwrap_or_fail_with_free!(8, &full_name, cam.queue_request(req));
    }

    // let _ = free_camera(&name);
    // Ok(())
    
}

// boy this is a crapload of typing weirdness to humor the lifetime restrictions of libcamera-rs
fn get_camera_by_name<'a>(camera_list: &mut CamIter<'a>, name: &str) -> 
            VisResult<Option<(Camera<'a>, String)>> {
    let mut cam: Option<Camera> = None;
    
    for camera in camera_list {
        if name == camera.id() || name == *camera.properties().get::<properties::Model>().unwrap() {
            if cam.is_some() {
                fail!(5, "camera name \"{}\" is not unique", name);
            }
            cam = Some(camera);
        }
    }

    Ok(cam.map(|c| {
        let name = c.id().to_string();
        (c, name)
    }))
}

fn translate_pixel_format(fourcc: FourCC) -> pf::PixelFormat {
    let format: PixelFormat = fourcc.to_string().parse().unwrap();
    pf::PixelFormat::new(u32::from_le_bytes(format.fourcc()), 0)
}

fn get_closest_size<'a>(cfg: &'a StreamConfigurationRef<'a>, config: &Launch) -> Option<Size> {
    let format = translate_pixel_format(config.format);
    let sizes = cfg.formats().sizes(format);

    if config.width.is_none() && config.height.is_none() {
        sizes.get(0).copied()
    } else if config.width.is_none() {
        // sort solely based on height
        let height = config.height.unwrap();
        sizes.into_iter().min_by_key(|s| {
            s.height.abs_diff(height)
        })
    } else if config.height.is_none() {
        // sort solely based on width
        let width = config.width.unwrap();
        sizes.into_iter().min_by_key(|s| {
            s.width.abs_diff(width)
        })
    } else {
        // sort based on lowest dw and dh 
        let width = config.width.unwrap();
        let height = config.height.unwrap();
        sizes.into_iter().min_by_key(|s| {
            s.width.abs_diff(width) + s.height.abs_diff(height)
        })
    }
}


fn use_camera(name: &str) -> VisResult<()> {
    let known_file = get_or_make_known_camera_file()?;
    let mut f = unwrap_or_fail!(1, File::options().create(true).append(true).open(known_file));

    unwrap_or_fail!(1, f.write(format!("{}\n", name).as_bytes()));
    Ok(())
}

fn free_camera(name: &str) -> VisResult<()> {
    let known_file = get_known_camera_file()?;
    let cams = get_used_cameras()?;
    let mut f = unwrap_or_fail!(1, File::options()
                                        .truncate(true)
                                        .create(true)
                                        .write(true)
                                        .open(known_file));
    for cam in cams.into_iter().filter(|n| n != name) {
        unwrap_or_fail!(1, f.write(format!("{}\n", cam).as_bytes()));
    }
    Ok(())
}
