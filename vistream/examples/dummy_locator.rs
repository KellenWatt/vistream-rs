
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
    c.start();

    let locator = DummyLocator::new();
    let locate_stream = LocateStream::launch("0.0.0.0:30202".parse().unwrap(), c, locator).unwrap();

    loop {}
    
    #[allow(unreachable)]
    locate_stream.stop().unwrap();
}

struct DummyLocator;

impl DummyLocator {
    fn new() -> DummyLocator {
        DummyLocator
    }
}

impl<F: PixelFormat, S: FrameSource<F>> Locate<F, S> for DummyLocator {
    fn locate(&mut self, source: &mut S) -> Result<Vec<LocationData>> {
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
