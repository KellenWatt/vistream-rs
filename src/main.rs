mod camera;
mod signal_thread;
mod thread;

// use libcamera::camera_manager::CameraManager;
use camera::{Camera, PixelFormat, FrameSource, CameraManager};


fn main() {

    println!("Creating manager");
    let mgr = CameraManager::new().unwrap();

    println!("creating wrapper");
    let mut cam = Camera::new(0, &mgr, PixelFormat::MJPEG).unwrap();

    // println!("Acquiring basic camera");
    // let cameras = mgr.cameras();
    // let cam = cameras.get(0).unwrap();
    // 
    // println!("creating wrapper");
    // let mut cam = Camera::new(&cam, PixelFormat::MJPEG).unwrap();
    
    println!("starting background capture");
    cam.start().unwrap();
    
    println!("getting frame");
    let frame = cam.get_frame();
    println!("frame get!");
    
    std::fs::write("out.jpg", &frame.data).unwrap();
}
