
use vistream::camera::{Camera, FrameSource, CameraConfig, Locate};
use vistream_protocol::stream::{LocationData};
use vistream::stream::{LocateStream};
use vistream::frame::{RGB, MJPG, Pixelate, PixelFormat, Frame};
use vistream::error::{Error, Result};

use std::net::{SocketAddr};

use turbojpeg::{Image, PixelFormat as JpegPixelFormat, Compressor};
use std::sync::Arc;

fn main() {
    let mut cfg = CameraConfig::default();
    cfg.server_exe("/home/bisonbots/dev/vistream/target/debug/vistream-camera-server");
    cfg.conn_timeout(std::time::Duration::from_secs(1));
    let mut c: Camera<MJPG> = Camera::new("lifecam", cfg).unwrap();
    // let mut c: Camera<MJPG> = Camera::new("lifecam", cfg).unwrap();
    // let mut c = MJPGSource::new(c);
    c.start();

    let locator = DummyLocator::new();
    // 
    // // let addr = SocketAddr::from("0.0.0.0:30202");
    let locate_stream = LocateStream::launch("0.0.0.0:30202".parse().unwrap(), c, locator);

    // std::thread::sleep(std::time::Duration::from_secs(3));
    loop {}
    
    locate_stream.stop().unwrap();

    // let mut f = None;
    // while f.is_none() {
    //     f = match c.get_frame() {
    //         Ok(frame) => {frame},
    //         Err(e) => {
    //             println!("{}", e);
    //             None
    //         }
    //     }
    //     // f = c.get_frame().unwrap();
    // }
    // 
    // println!("client: frame received");
    // 
    // let f = f.unwrap();
    // dbg!(f.width());
    // dbg!(f.height());
    // 
    // std::fs::write("current.jpg", f.bytes()).unwrap();
}

struct MJPGSource<S: FrameSource<RGB>> {
    source: S,
    last_frame: usize,
    compressor: Compressor,
}

impl<S: FrameSource<RGB>> MJPGSource<S> {
    fn new(source: S) -> MJPGSource<S> {
        MJPGSource {
            source,
            last_frame: 0,
            compressor: Compressor::new().unwrap(), // Do not leave this
        }
    }
}

impl<S: FrameSource<RGB>> FrameSource<MJPG> for MJPGSource<S> {
    fn get_frame(&mut self) -> Result<Option<Arc<Frame<MJPG>>>> {
        let Some(frame) = self.source.get_frame()? else {
            println!("no frame!");
            return Ok(None);
        };

        let width = frame.width();
        let height = frame.height();

        let image: Image<&[u8]> = Image {
            pixels: &frame.bytes(),
            width: width,
            height: height,
            pitch: frame.width() * RGB::byte_count(),
            format: JpegPixelFormat::RGB,
        };

        let Ok(image) = self.compressor.compress_to_owned(image) else {
            return Err(Error::Unknown);
        };


        let out = Frame::new(&*image, width, height);
        let out = Arc::new(out);
        self.last_frame = self.source.last_frame_id();
        Ok(Some(out))
    }

    fn start(&mut self) -> Result<()> {
        self.source.start()
    }
    
    fn stop(&mut self) -> Result<()> {
        self.source.stop()
    }

    fn last_frame_id(&self) -> usize {
        self.last_frame
    }
}
// pub trait Locate<F: frame::PixelFormat, S: FrameSource<F>> {
//     fn locate(&mut self, source: &S) -> Result<Vec<LocationData>>;
//     fn locate_once(&mut self, source: &S) -> Result<Option<LocationData>> {
//         Ok(self.locate(source)?.get(0).copied())
//     }
//     fn contains_target(&mut self, source: &S) -> Result<bool> {
//         Ok(self.locate(source)?.len() > 0)
//     }
// }
// use std::marker::PhantomData
struct DummyLocator;

impl DummyLocator {
    fn new() -> DummyLocator {
        DummyLocator
    }
}

impl<F: PixelFormat, S: FrameSource<F>> Locate<F, S> for DummyLocator {
    fn locate(&mut self, source: &S) -> Result<Vec<LocationData>> {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let mut builder = LocationData::builder();
        builder.x(0.1);
        builder.y(0.2);
        builder.width(160.0);
        builder.height(90.0);
        builder.roll(180.0);
        builder.id(u32::MAX);
        let Ok(dummy) = builder.build() else {
            return Err(Error::Unknown);
        };

        Ok(vec![dummy])
    }
}
