use libcamera::{
    camera::{CameraConfigurationStatus, Camera},
    camera_manager::CameraManager,
    framebuffer::AsFrameBuffer,
    framebuffer_allocator::{FrameBuffer, FrameBufferAllocator},
    framebuffer_map::MemoryMappedFrameBuffer,
    pixel_format as pf,
    properties,
    stream::StreamRole,
};

mod parser;
use clap::{Parser};

use vistream_protocol::camera::{PixelFormat};

use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};
use std::os::linux::net::{SocketAddrExt};

use std::path::{PathBuf};
use std::fs::{self, File};
use std::collections::HashMap;
use std::io::{self, BufReader, BufRead, Write};

fn fail(code: i32, msg: &str) {
    let current_exe = std::env::current_exe().unwrap();
    eprintln!("{}: {}", current_exe.file_name().unwrap().to_string_lossy(), msg);
    std::process::exit(code);
}

fn main() {
    std::env::set_var("LIBCAMERA_LOG_LEVELS", "*:4");
    let args = parser::Cli::parse();

    match args.command {
        parser::Command::List => {
            // list cameras as "model (id)" (possibly with libcamera index?)
            for name in get_camera_names() {
                println!("{}", name);
            }
        }
        parser::Command::Launch(launch) => {
            // This is where we do the actual server stuff
        }
        parser::Command::Alias(alias) => {
            // TODO figure out what it takes to list and rm aliases
            let res = create_alias(alias);
            if res.is_err() {
                fail(3, &format!("Unable to create alias ({})", res.unwrap_err()));
            }
        }
    }

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

fn get_alias_file() -> PathBuf {
    let home = get_or_make_home();
    let alias_file = home.join("names");
    alias_file
}

fn load_aliases() -> io::Result<HashMap<String, String>> {
    let alias_file = get_alias_file();

    if !alias_file.is_file() {
        return Ok(HashMap::new());
    }
    let file = File::open(alias_file)?;
    let file = BufReader::new(file);
    
    Ok(file.lines().filter_map(|line| {
        let line = line.unwrap();
        let (k, v) = line.split_once("=")?;
        Some((k.to_string(), v.to_string()))
    }).fold(HashMap::new(), |mut h, (k, v)| {
        h.insert(k, v);
        h
    }))
}

fn save_aliases(h: HashMap<String, String>) -> io::Result<()> {
    let alias_file = get_alias_file();

    let mut file = File::create(alias_file)?;

    for (k, v) in h.iter() {
        file.write(format!("{}={}", k, v).as_bytes())?;
    }
    Ok(())
}

fn create_alias(alias: parser::Alias) -> io::Result<()> {
    let mut aliases = load_aliases()?;

    if alias.name.contains("=") {
        fail(5, "alias connot contain '='");
    }

    let _ = aliases.insert(alias.name, alias.alias);

    save_aliases(aliases)
}

fn resolve_alias(name: String) -> io::Result<String> {
    let aliases = load_aliases()?;

    Ok(aliases.get(&name).unwrap_or(&name).to_owned())
}

fn get_camera_names() -> Vec<String> {
    let mgr = CameraManager::new().unwrap();
    let cameras = mgr.cameras();

    let mut names = vec![];
    for i in 0..cameras.len() {
        let cam = cameras.get(i).unwrap();
        let name = pretty_camera_name(&cam);
        names.push(name);
    }

    names
}

fn pretty_camera_name(cam: &Camera<'_>) -> String {
    let id = cam.id();
    let model = cam.properties().get::<properties::Model>().unwrap();

    format!("{} ({})", *model, id)
}
