use serde::{Serialize, Deserialize};
use serde_repr::{Serialize_repr, Deserialize_repr};

use std::str::FromStr;


#[derive(Debug, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum PixelFormat {
    RGB,
    BGR,
    RGBA,
    BGRA,
    YUYV,
    MJPEG,
}

impl PixelFormat {
    /// Returns the FourCC representation used for this pixel format. This information is
    /// largely presented to be informational, but if there is a use for it, it's here.
    ///
    /// As a note, the representations for any RGB/BGR are backwards because of how
    /// libcamera and likely the underlying Linux kernel handle byte order. If your R and B
    /// are swapped, switch to other option, and that should fix your issue.
    pub fn fourcc(&self) -> [u8; 4] {
        match self {
            PixelFormat::RGB => *b"BG24",
            PixelFormat::BGR => *b"RG24",
            PixelFormat::RGBA => *b"BA24",
            PixelFormat::BGRA => *b"RA24",
            PixelFormat::YUYV => *b"YUYV",
            PixelFormat::MJPEG => *b"MJPG"
        }
    }
}

impl FromStr for PixelFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<PixelFormat, Self::Err> {
        match s {
            "BG24" => Ok(PixelFormat::RGB),
            "RG24" => Ok(PixelFormat::BGR),
            "BA24" => Ok(PixelFormat::RGBA),
            "RA24" => Ok(PixelFormat::BGRA),
            "YUYV" => Ok(PixelFormat::YUYV),
            "MJPG" => Ok(PixelFormat::MJPEG),
            _ => Err(format!("not a recognized fourcc code ({})", s)),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Frame {
    format: PixelFormat,
    width: usize,
    height: usize,
    data: Box<[u8]>,
}


#[derive(Serialize, Deserialize)]
enum ClientMessage {
    Start,
    Stop,
    Disconnect,
}

