pub mod camera;
pub mod frame;
pub mod stream;
pub mod error;
pub mod transform;
pub mod client;

pub use crate::camera::{Camera, CameraConfig, FrameSource, Locate};
pub use vistream_protocol::stream::{LocationData};
pub use crate::frame::{Frame, Pixelate};

#[cfg(feature = "ws")]
pub mod ws;

/// This function is not "unsafe" in the Rust sense. Rather, the `unsafe` 
/// here is intended as a marker for a function that should never be called
/// without consideration. In other words, this is abusing the Rust compiler 
/// to get just a little more safety.
pub unsafe fn init() -> crate::error::Result<()> {
    use vistream_protocol::fs::{get_known_camera_file, get_camera_pid_file};
    use std::fs::remove_file;
    let cam_file = get_known_camera_file().unwrap();
    remove_file(cam_file)?;
    let pid_file = get_camera_pid_file().unwrap();
    remove_file(pid_file)?;
    Ok(())
}
