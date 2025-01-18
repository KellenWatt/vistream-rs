use serde::{Serialize, Deserialize};
use serde_repr::{Serialize_repr, Deserialize_repr};

use std::str::FromStr;


#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
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
        let s = s.to_uppercase();
        // Yes, these are technically backward, however, these are the "true" FourCC codes
        // corresponding to the formats, and the fourcc impl deals with the weirdness
        match s.as_str() {
            "RG24" => Ok(PixelFormat::RGB),
            "BG24" => Ok(PixelFormat::BGR),
            "RA24" => Ok(PixelFormat::RGBA),
            "BA24" => Ok(PixelFormat::BGRA),
            "YUYV" => Ok(PixelFormat::YUYV),
            "MJPG" => Ok(PixelFormat::MJPEG),
            _ => Err(format!("not a recognized fourcc code ({})", s)),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Frame<'a> {
    pub format: PixelFormat,
    pub width: u32,
    pub height: u32,
    #[serde(with = "serde_bytes")]
    pub data: &'a [u8],
}


#[derive(Clone, Copy, Debug, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum ClientMessage {
    Start,
    Stop,
    Disconnect,
    Status,
}

impl ClientMessage {
    pub fn id(&self) -> u8 {
        match self {
            ClientMessage::Start => 0,
            ClientMessage::Stop => 1,
            ClientMessage::Disconnect => 2,
            ClientMessage::Status => 3,
        }
    }

    pub fn from_id(id: u8) -> Option<ClientMessage> {
        match id {
            0 => Some(ClientMessage::Start),
            1 => Some(ClientMessage::Stop),
            2 => Some(ClientMessage::Disconnect),
            3 => Some(ClientMessage::Status),
            _ => None
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Status {
    pub enabled: bool,
    pub healthy: bool,
    pub format: PixelFormat,
    pub width: usize,
    pub height: usize,
}

#[derive(Serialize, Deserialize)]
pub struct CameraList {
    pub cameras: Vec<CameraListing>,
}

#[derive(Serialize, Deserialize)]
pub struct CameraListing {
    pub name: String,
    pub id: String,
    pub acquired: bool,
}

impl std::fmt::Display for CameraListing {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} ({})", self.name, self.id)?;
        if self.acquired {
            write!(f, " (acquired)")?;
        }
        Ok(())
    }
}
