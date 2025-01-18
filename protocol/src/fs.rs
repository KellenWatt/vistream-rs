use std::path::{PathBuf};
use std::fs::{self, File, read_to_string};
use std::collections::HashMap;
use std::io::{BufReader, BufRead, Write};

type VisResult<T> = std::result::Result<T, u8>;

macro_rules! fail {
    ($code: expr, $e:expr) => {
        let current_exe = std::env::current_exe().unwrap();
        eprintln!("{}: {}", current_exe.file_name().unwrap().to_string_lossy(), $e);
        // std::process::exit($code);
        return Err($code);
    };
    ($code:expr, $($tts:tt)*) => {
        let current_exe = std::env::current_exe().unwrap();
        eprintln!("{}: {}", current_exe.file_name().unwrap().to_string_lossy(), format!($($tts)*));
        // std::process::exit($code);
        return Err($code);
    };
}

macro_rules! unwrap_or_fail {
    ($code: expr, $value: expr) => {
        match $value {
            Ok(v) => v,
            Err(e) => {
                fail!($code, e.to_string());
            }
        }
    }
}

pub fn get_or_make_home() -> VisResult<PathBuf> {
    let home = PathBuf::from(std::env::var("HOME").unwrap());
    let home = home.join(".vistream");

    if !home.is_dir() && home.exists() {
        fail!(2, "~/.vistream already exists, but it isn't a directory");
    }
    if !home.exists() {
        if fs::create_dir(&home).is_err() {
            fail!(2, "could not make a home for vistream");
        }
    }
    Ok(home)
}

pub fn get_camera_home() -> VisResult<PathBuf> {
    let home = get_or_make_home()?;

    Ok(home.join("camera"))
}

pub fn get_alias_file() -> VisResult<PathBuf> {
    let home = get_or_make_home()?;
    let alias_file = home.join("names");
    Ok(alias_file)
}

pub fn load_aliases() -> VisResult<HashMap<String, String>> {
    let alias_file = get_alias_file()?;

    if !alias_file.is_file() {
        return Ok(HashMap::new());
    }
    let file = match File::open(alias_file) {
        Ok(file) => file,
        Err(e) => {
            fail!(3, e);
        }
    };
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

pub fn save_aliases(h: HashMap<String, String>) -> VisResult<()> {
    let alias_file = get_alias_file()?;

    let mut file = match File::create(alias_file) {
        Ok(file) => file,
        Err(e) => {fail!(3, e);}
    };

    for (k, v) in h.iter() {
        unwrap_or_fail!(4, file.write(format!("{}={}", k, v).as_bytes()));
    }
    Ok(())
}

pub fn resolve_alias(name: &str) -> VisResult<String> {
    let aliases = load_aliases()?;
    let name = name.to_string();

    Ok(aliases.get(&name).unwrap_or(&name).to_owned())
}

pub fn get_known_camera_file() -> VisResult<PathBuf> {
    let cam_home = get_camera_home()?;
    let known_file = cam_home.join("acquired");
    Ok(known_file)
}

pub fn get_or_make_known_camera_file() -> VisResult<PathBuf> {
    let cam_home = get_camera_home()?;
    if !cam_home.is_dir() {
        unwrap_or_fail!(1, std::fs::create_dir(&cam_home));
    }
    let known_file = cam_home.join("acquired");

    Ok(known_file)
}

pub fn get_used_cameras() -> VisResult<Vec<String>> {
    let known_file = get_known_camera_file()?;

    Ok(match read_to_string(known_file) {
        Ok(s) => s.lines().map(str::to_string).collect(),
        Err(_) => Vec::new(),
    })
}

pub fn is_camera_used(name: &str) -> VisResult<bool> {
    let known_cams = get_used_cameras()?;
    Ok(known_cams.contains(&name.to_string()))
}
