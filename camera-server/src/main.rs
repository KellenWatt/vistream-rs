use libcamera::{
    camera::CameraConfigurationStatus,
    camera_manager::CameraManager,
    framebuffer::AsFrameBuffer,
    framebuffer_allocator::{FrameBuffer, FrameBufferAllocator},
    framebuffer_map::MemoryMappedFrameBuffer,
    pixel_format as pf,
    // properties,
    stream::StreamRole,
};

use argparse::{ArgumentParser, StoreTrue, Store};

use vistream_protocol::camera::{PixelFormat};

use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};
use std::os::linux::net::{SocketAddrExt};

use std::path::{PathBuf, Path};
use std::fs;

fn fail(code: i32, msg: &str) {
    eprintln!("{}", msg);
    std::process::exit(code);
}

fn main() {
    let home = get_or_make_home();

    println!("{}", home.display());


    let mut cam_name: Option<String> = None;
    let mut cam_number: Option<u32> = None;
    let mut format = "RGB".to_string();
    let mut width: Option<usize> = None;
    let mut height: Option<usize> = None;
    {
        let mut parser = argparse::ArgumentParser::new();
        parser.set_description("Camera server using the vistream camera-server protocol");

        parser.refer(&mut cam_name)
            .add_option(&["--name"], argparse::StoreOption, "Attempts to acquire the camera named NAME");

        parser.refer(&mut cam_number)
            .add_option(&["--index"], argparse::StoreOption, "Attempts to acquire the camera at INDEX (according to libcamera ordering)");

        parser.refer(&mut format)
            .add_option(&["--format"], argparse::Store, "Sets the output pixel format (FourCC code)");

        parser.refer(&mut width)
            .add_option(&["--width"], argparse::StoreOption, "Requests output be WIDTH pixels  wide");

        parser.refer(&mut height)
            .add_option(&["--height"], argparse::StoreOption, "Requests output be HEIGHT pixels high");

        parser.parse_args_or_exit();
    }

    if cam_name.is_none() && cam_number.is_none() {
        fail(1, "No camera specified");
    }
    let format: PixelFormat = format.parse().unwrap_or_else(|e: <PixelFormat as std::str::FromStr>::Err| {
        fail(3, &e.to_string());
        unreachable!();
    });
}

fn get_or_make_home() -> PathBuf {
    let home = PathBuf::from(std::env::var("HOME").unwrap());
    let home = home.join(".vistream");

    if !home.is_dir() && home.exists() {
        fail(2, "~/.vistream already exists, but it isn't a directory");
    }
    if !home.exists() {
        if fs::create_dir(&home).is_err() {
            fail(2, "could not make a home for vistream");
        }
    }
    home
}

fn get_camera_home() -> PathBuf {
    let home = get_or_make_home();

    home.join("camera")
}
