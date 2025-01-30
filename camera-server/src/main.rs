use libcamera::{
    camera_manager::CameraManager,
    properties,
};
mod parser;
mod server;
mod shared;
use crate::shared::*;
use clap::{Parser};

use vistream_protocol::camera::{CameraList, CameraListing};
use vistream_protocol::fs::*;
use serde::{Serialize};
use rmp_serde::{Serializer};
use std::io::{BufWriter, Write, stdout};

// use std::path::{PathBuf};
// use std::fs::{self, File};
// use std::collections::HashMap;
// use std::io::{self, BufReader, BufRead, Write};

// This feels kind of like a dirty hack, but it does the job
fn main()-> std::process::ExitCode {
    match pseudo_main() {
        Ok(_) => std::process::ExitCode::SUCCESS,
        Err(code) => std::process::ExitCode::from(code),
    }
}

fn pseudo_main() -> VisResult<()> {
    std::env::set_var("LIBCAMERA_LOG_LEVELS", "*:4");
    let args = parser::Cli::parse();

    match args.command {
        parser::Command::List(list) => {
            // list cameras as "model (id)" (possibly with libcamera index?)
            if list.piped {
                let mut write = BufWriter::new(stdout());
                let cams = get_camera_names()?.cameras;
                unwrap_or_fail!(11, cams.serialize(&mut Serializer::new(&mut write)));
                unwrap_or_fail!(11, write.flush());
            } else {
                for cam in get_camera_names()?.cameras {
                    println!("{}", cam);
                }
            }
        }
        parser::Command::Launch(launch) => {
            // This is where we do the actual server stuff
            server::launch(launch)?;
        }
        parser::Command::Alias(alias) => {
            // TODO figure out what it takes to list and rm aliases
            match alias {
                parser::Alias::List => {
                    let aliases = load_aliases()?;
                    for (alias, name) in aliases.into_iter() {
                        println!("{}={}", alias, name);
                    }
                }
                parser::Alias::Remove(rm) => {
                    let _ = remove_alias(rm)?;
                }
                parser::Alias::Create(ca) => {
                    let _ = create_alias(ca)?;
                }
            }
        }
        parser::Command::Resolve(resolve) => {
            let cam_id = full_resolve_name(resolve.name)?;
            println!("{}", cam_id);
        }

        parser::Command::Check(check) => {
            let name = full_resolve_name(check.name)?;
            if is_camera_used(&name)? {
                if check.quiet {
                    fail!(255, "");
                } else {
                    fail!(255, "{} is already acquired", name);
                }
            } else {
                if !check.quiet {
                    println!("{} is free", name);
                }
            }
        }

        parser::Command::Stop(stop) => {
            let name = full_resolve_name(stop.name)?;
            let pids = get_camera_pids()?;
            let Some(pid) = pids.get(&name)  else {
                fail!(20, "Camera not found (is it running?)");
            };
            match std::process::Command::new("kill").arg(pid.to_string()).output() {
                Ok(_) => {/* no-op */}
                Err(_) => {fail!(20, "Camera could not be killed (is it running?)");}
            }
        }
    }

    Ok(())
}

fn get_camera_names() -> VisResult<CameraList> {
    let mgr = CameraManager::new().unwrap();
    let cameras = mgr.cameras();

    let used_cameras = get_used_cameras()?;

    let mut names = vec![];
    for i in 0..cameras.len() {
        let cam = cameras.get(i).unwrap();
        let listing = CameraListing {
            name: cam.properties().get::<properties::Model>().unwrap().to_string(),
            id: cam.id().to_string(),
            acquired: used_cameras.contains(&cam.id().to_string()),
        };
        names.push(listing);
    }

    Ok(CameraList{cameras: names})
}

pub fn create_alias(alias: parser::CreateAlias) -> VisResult<()> {
    let mut aliases = load_aliases()?;

    if alias.name.contains("=") {
        fail!(5, "alias connot contain '='");
    }

    if aliases.contains_key(&alias.alias) && !alias.update {
        fail!(5, "alias \"{}\" already exists ({})", alias.alias, alias.name);
    }

    let _ = aliases.insert(alias.alias, alias.name);

    save_aliases(aliases)
}

pub fn remove_alias(alias: parser::RemoveAlias) -> VisResult<()> {
    let mut aliases = load_aliases()?;
    
    if aliases.remove(&alias.alias).is_none() {
        fail!(5, "\"{}\" is not an alias", alias.alias);
    }
    save_aliases(aliases)
}

pub fn full_resolve_name(src_name: String) -> VisResult<String> {
    let name = resolve_alias(&src_name)?;
    let mgr = CameraManager::new().unwrap();
    let cameras = mgr.cameras();

    let mut cam_id = None;

    for i in 0..cameras.len() {
        let cam = cameras.get(i).unwrap();
        let cam_name = cam.properties().get::<properties::Model>().unwrap().to_string();
        if cam_name == name || cam.id() == name {
            if cam_id.is_some() {
                fail!(12, "\"{}\" is not unambiguous", name);
            }
            cam_id = Some(cam.id().to_string());
        }
    }
    if cam_id.is_none() {
        fail!(12, "\"{}\" is not recognized by vistream", src_name);
    }

    Ok(cam_id.unwrap())
}
