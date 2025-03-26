use crate::camera::{Worker, FrameSource};
use crate::frame::{Frame, MJPG};
use crate::error::{Result, Error};
use vistream_protocol::stream::{ClientMessage, Frame as ProtoFrame, LocationData};
use std::net::{TcpStream, SocketAddr};
use std::io::{Write, BufReader};

use std::sync::{Arc, RwLock};
use std::sync::atomic::{Ordering, AtomicUsize};

use rmp_serde::decode::{Deserializer};
use serde::Deserialize;

pub struct FrameClient {
    worker: Worker,
    control: TcpStream,
    last_frame: Arc<RwLock<Option<Arc<Frame<MJPG>>>>>,
    last_frame_id: Arc<AtomicUsize>,
}

impl FrameClient {
    pub fn connect(addr: SocketAddr) -> Result<FrameClient> {
        let socket = TcpStream::connect(addr)?;
        // Dear future self: If something is breaking in the FrameClient, it's probably because of
        // this line. Yes, that means you need to actually improve your socket handling.
        let _ = socket.set_read_timeout(Some(std::time::Duration::from_secs(1)));
        let control = socket.try_clone()?;

        let last_frame = Arc::new(RwLock::new(None));
        let last_frame_id = Arc::new(AtomicUsize::new(0));
        let worker_frame = last_frame.clone();
        let worker_frame_id = last_frame_id.clone();
        let worker = Worker::spawn(move |kill_flag| {
            let mut socket = socket;
            let mut deserializer = Deserializer::new(&mut socket);
            
            while !kill_flag.load(Ordering::Acquire) {
                let proto_frame = ProtoFrame::deserialize(&mut deserializer)?;
                let data = proto_frame.data;
                let frame = Frame::new(data, proto_frame.width as usize, proto_frame.height as usize);
                *worker_frame.write().unwrap() = Some(Arc::new(frame));
                worker_frame_id.fetch_add(1, Ordering::AcqRel);
            }

            Err(Error::Unknown)
        });

        Ok(FrameClient {
            worker,
            control,
            last_frame,
            last_frame_id,
        })
    }
}

impl Drop for FrameClient {
    fn drop(&mut self) {
        self.worker.join();
        let _ = self.control.shutdown(std::net::Shutdown::Both);
    }
}

impl FrameSource<MJPG> for FrameClient {
    fn get_frame(&mut self) -> Result<Option<Arc<Frame<MJPG>>>> {
        // If the frame_worker has stopped for any reason, return nothing,
        // except for the first time, where it returns the error causing it.
        if self.worker.is_finished() {
            if self.worker.is_joinable() {
                // ignore the error, since something has already gone wrong
                let _ = self.control.write(&[ClientMessage::Disconnect.id()]);
                self.control.flush()?;
                return match self.worker.join() {
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
                self.worker.kill();
                let _ = self.control.write(&[ClientMessage::Disconnect.id()]);
                self.control.flush()?;
                match self.worker.join() {
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

#[allow(dead_code)]
pub struct LocateClient {
    worker: Worker,
    control: TcpStream,
    last_data: Arc<RwLock<Option<LocationData>>>,
    last_data_id: Arc<AtomicUsize>,
}

#[allow(dead_code)]
impl LocateClient {
    pub fn connect(addr: SocketAddr) -> Result<LocateClient> {
        let socket = TcpStream::connect(addr)?;
        let _ = socket.set_read_timeout(Some(std::time::Duration::from_secs(1)));
        let control = socket.try_clone()?;

        let last_data = Arc::new(RwLock::new(None));
        let last_data_id = Arc::new(AtomicUsize::new(0));
        let worker_data = last_data.clone();
        let worker_data_id = last_data_id.clone();
        
        let worker = Worker::spawn(move |kill_flag| {
            let socket = serde_json::de::IoRead::new(BufReader::new(socket));
            let mut deserializer = serde_json::Deserializer::new(socket);
            
            while !kill_flag.load(Ordering::Acquire) {
                let loc_data = LocationData::deserialize(&mut deserializer).unwrap();
                *worker_data.write().unwrap() = Some(loc_data);
                worker_data_id.fetch_add(1, Ordering::AcqRel);
            }

            Err(Error::Unknown)
        });

        Ok(LocateClient {
            worker,
            control,
            last_data,
            last_data_id,
        })
    }
    
    pub fn start(&mut self) -> Result<()> {
        self.control.write(&[ClientMessage::Start.id()])?;
        self.control.flush()?;
        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        self.control.write(&[ClientMessage::Stop.id()])?;
        self.control.flush()?;
        Ok(())
    }

    pub fn last_data_id(&self) -> usize {
        self.last_data_id.load(Ordering::Acquire)
    }
}




