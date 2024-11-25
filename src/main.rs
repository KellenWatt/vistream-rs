mod camera;
mod signal_thread;
mod thread;

use std::cell::OnceCell;
use libcamera::camera_manager::CameraManager;
use camera::{Camera, PixelFormat, FrameSource};

lazy_static::lazy_static! {
    static ref mgr: CameraManager = CameraManager::new().unwrap();
}

fn main() {

    println!("Acquiring basic camera");
    let cameras = mgr.cameras();
    let cam = cameras.get(0).unwrap();

    println!("creating wrapper");
    let mut cam = Camera::new(&cam, PixelFormat::MJPEG).unwrap();

    println!("starting background capture");
    cam.start().unwrap();

    println!("getting frame");
    let frame = cam.get_frame();
    println!("frame get!");
    
    std::fs::write("out.jpg", &frame.data).unwrap();
}
