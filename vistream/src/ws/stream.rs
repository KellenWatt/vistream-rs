use crate::camera::{FrameSource, Locate, Worker, Camera, CameraConfig};
use crate::frame::{PixelFormat, MJPG, Pixelate};
use crate::error::{Result, Error};
use vistream_protocol::stream::{ClientMessage, Status, Frame as ProtoFrame};

use std::net::{TcpListener, SocketAddr, TcpStream};
use std::io::{self, Write, Read};
// use std::sync::{Arc, RwLock};
use std::sync::atomic::{Ordering};

use tungstenite as ws;
use tungstenite::{WebSocket};

#[allow(dead_code)]
struct Connection {
    socket: WebSocket<TcpStream>,
    // serializer: Serializer<UnixStream>,
    healthy: bool,
    active: bool,
}

impl Drop for Connection {
    fn drop(&mut self) {
        if let Ok(_) = self.socket.close(None) {
            while self.socket.read().is_ok() {}
        }
    }
}

#[allow(dead_code)]
impl Connection {
    fn new(conn: WebSocket<TcpStream>) -> Connection {
        Connection {
            socket: conn,
            healthy: true,
            active: false,
        }
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

    fn can_read(&self) -> bool {
        self.socket.can_read()
    }

    fn read(&mut self) -> Result<ws::Message> {
        self.socket.read().map_err(|e| e.into())
    }
    
    fn write_text<S: AsRef<str>>(&mut self, payload: S) -> Result<()> {
        self.socket.write(payload.as_ref().into()).map_err(|e| e.into())
    }

    fn write_bin<B: AsRef<[u8]>>(&mut self, payload: B) -> Result<()> {
        self.socket.write(payload.as_ref().into()).map_err(|e| e.into())
    }

}

// impl Write for Connection {
//     fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
//         self.socket.write(buf)
//     }
//     fn flush(&mut self) -> io::Result<()> {
//         self.socket.flush()
//     }
// }
// 
// impl Read for Connection {
//     fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
//         self.socket.read(buf)
//     }
// }

pub struct WSFrameStream {
    worker: Worker,
}

impl WSFrameStream {
    pub fn launch<S, F>(addr: SocketAddr, source: S) -> Result<WSFrameStream>
    where S: FrameSource<F> + Send + 'static, 
          F: PixelFormat {
        let socket = TcpListener::bind(addr)?;
        socket.set_nonblocking(true)?;
        let worker = Worker::spawn(move |kill_flag| {
            let mut source = source;
            let mut connections = Vec::new();

            while !kill_flag.load(Ordering::Acquire) {
                match socket.accept() {
                    Ok((stream, _addr)) => {
                        let ws = ws::accept(stream).map_err(|_| Error::Handshake)?;
                        connections.push(Connection::new(ws))
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {/* no-op */}
                    Err(e) => {return Err(e.into());}
                }
            }
            

            todo!()
        });
        Ok(WSFrameStream {
            worker,
        })
    }

    pub fn stop(mut self) -> Result<()> {
        match self.worker.join() {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }
}
