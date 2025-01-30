use crate::camera::{FrameSource, Locate, Worker};
use crate::frame::{PixelFormat, MJPG, Pixelate};
use crate::error::{Result, Error};
use vistream_protocol::stream::{ClientMessage, Status, Frame as ProtoFrame};

use std::net::{TcpListener, SocketAddr, TcpStream};
use std::io::{self, Write, Read};
// use std::sync::{Arc, RwLock};
use std::sync::atomic::{Ordering};

// use std::marker::PhantomData;
use serde::{Serialize};

// FIXME fix error type later
pub fn make_response<S: Serialize>(kind: ClientMessage, data: S) -> std::result::Result<Vec<u8>, ()> {
    let mut out = vec![kind.id()];
    let Ok(mut buf) = serde_json::to_vec(&data) else {
        return Err(());
    }; 
    let Ok(len) = u32::try_from(buf.len()) else {
        return Err(());
    };
    out.extend(len.to_be_bytes());
    out.append(&mut buf);
    Ok(out)
}

#[allow(dead_code)]
struct Connection {
    socket: TcpStream,
    #[allow(dead_code)]
    addr: SocketAddr,
    // serializer: Serializer<UnixStream>,
    healthy: bool,
    active: bool,
}

#[allow(dead_code)]
impl Connection {
    fn new(conn: (TcpStream, SocketAddr)) -> Connection {
        
        conn.0.set_nonblocking(true).unwrap();
        Connection {
            socket: conn.0,
            addr: conn.1,
            // serializer: Serializer::new(conn.0),
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

    fn try_read(&mut self, buf: &mut [u8]) -> Result<Option<usize>> {
        match self.socket.read(buf) {
            Ok(n) => Ok(Some(n)),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

impl Write for Connection {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.socket.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.socket.flush()
    }
}

impl Read for Connection {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.socket.read(buf)
    }
}

pub struct LocateStream {
    worker: Worker,
}

impl LocateStream {
    pub fn launch<F, S, L>(addr: SocketAddr, source: S, locator: L) -> LocateStream 
    where F: PixelFormat,
          S: FrameSource<F> + Send + 'static, 
          L: Locate<F, S> + Send + 'static {
        let worker = Worker::spawn(move |kill_flag| {
            // let mut source = source;
            let mut locator = locator;
            let socket = TcpListener::bind(addr)?;
            socket.set_nonblocking(true)?;
            let mut connections = Vec::new();
        
            while !kill_flag.load(Ordering::Acquire) {
                match socket.accept() {
                    Ok(conn) => {
                        println!("connection get! from {}", conn.1);
                        connections.push(Connection::new(conn));
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        // do nothing
                    }
                    Err(e) => {
                        return Err(e.into());
                    }
                } 

                // prune connections
                connections = connections.into_iter().filter(|conn| conn.is_healthy()).collect();

                if connections.is_empty() {
                    std::thread::sleep(std::time::Duration::from_millis(16)); // ~1 frame @ 60fps
                    continue;
                }

                let loc = locator.locate(&source)?;
                // seriously, fix this error
                let loc_buf = make_response(ClientMessage::Start, loc).map_err(|_| Error::Unknown)?;

                for conn in connections.iter_mut() {
                    // listen for messages from, respond accordingly
                    // if conn active and healthy, send loc
                    let mut buf = [0u8];
                    match conn.try_read(&mut buf) {
                        Ok(Some(1)) => {
                            // respond as appropriate
                            match ClientMessage::from_id(buf[0]) {
                                Some(ClientMessage::Start) => {println!("client starting"); conn.activate()},
                                Some(ClientMessage::Stop) => {println!("client stopping"); conn.deactivate()},
                                Some(ClientMessage::Disconnect) => {println!("client disconnecting"); conn.poison()},
                                Some(ClientMessage::Status) => {
                                    println!("client requesting status");
                                    let resp = make_response(ClientMessage::Status, Status {
                                        enabled: conn.is_active(),
                                        healthy: conn.is_healthy(),
                                        framerate: 0.0,
                                    }).map_err(|_| Error::Unknown)?;
                                    conn.write(&resp)?;
                                }
                                None => {println!("don't know what I got")/* silently ignore */}
                            }
                        }
                        Ok(Some(0)) => {/* ignore? */}
                        Ok(Some(_)) => unreachable!(),
                        Ok(None) => {/* do nothing */}
                        Err(_) => {println!("something went wrong with the connection");conn.poison();}
                    }

                    if !conn.is_active() {
                        continue;
                    }

                    conn.write(&loc_buf)?; 
                }
            }

            Ok(())
        });
        LocateStream {
            worker
        }
    }

    pub fn stop(mut self) -> Result<()> {
        match self.worker.join() {
            Some(err) => Err(err),
            None => Ok(())
        }
    }
}

pub struct FrameStream{
    worker: Worker,
}

impl FrameStream {
    pub fn launch<S>(addr: SocketAddr, source: S) -> FrameStream
    where S: FrameSource<MJPG> + Send + 'static {
        let worker = Worker::spawn(move |kill_flag| {
            let mut source = source;
            let socket = TcpListener::bind(addr)?;
            socket.set_nonblocking(true)?;
            let mut connections = Vec::new();
        
            while !kill_flag.load(Ordering::Acquire) {
                match socket.accept() {
                    Ok(conn) => {
                        connections.push(Connection::new(conn));
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        // do nothing
                    }
                    Err(e) => {
                        return Err(e.into());
                    }
                } 

                // prune connections
                connections = connections.into_iter().filter(|conn| conn.is_healthy()).collect();

                if connections.is_empty() {
                    std::thread::sleep(std::time::Duration::from_millis(16)); // ~1 frame @ 60fps
                    continue;
                }
               
                let frame = source.get_frame()?;
                let frame_buf = match frame {
                    Some(frame) => {
                        let frame = ProtoFrame {
                            width: frame.width() as u32,
                            height: frame.height() as u32,
                            data: frame.bytes(),
                        };
                        make_response(ClientMessage::Start, frame).map_err(|_| Error::Unknown)?
                    }
                    None => {
                        Vec::new()
                    }
                };
                
                for conn in connections.iter_mut() {
                    // listen for messages from, respond accordingly
                    // if conn active and healthy, send loc
                    let mut buf = [0u8];
                    match conn.try_read(&mut buf)? {
                        Some(1) => {
                            // respond as appropriate
                            match ClientMessage::from_id(buf[0]) {
                                Some(ClientMessage::Start) => conn.activate(),
                                Some(ClientMessage::Stop) => conn.deactivate(),
                                Some(ClientMessage::Disconnect) => conn.poison(),
                                Some(ClientMessage::Status) => {
                                    let resp = make_response(ClientMessage::Status, Status {
                                        enabled: conn.is_active(),
                                        healthy: conn.is_healthy(),
                                        framerate: 0.0,
                                    }).map_err(|_| Error::Unknown)?;
                                    conn.write(&resp)?;
                                }
                                None => {/* silently ignore */}
                            }
                        }
                        Some(0) => {/* ignore? */}
                        Some(_) => unreachable!(),
                        None => {/* do nothing */}
                    }

                    if !conn.is_active() {
                        continue;
                    }

                    if frame_buf.len() > 0 {
                        conn.write(&frame_buf)?; 
                    }
                }
            }

            Ok(())
        });
        FrameStream {
            worker
        }
    }
    
    pub fn stop(mut self) -> Result<()> {
        match self.worker.join() {
            Some(err) => Err(err),
            None => Ok(())
        }
    }
}
