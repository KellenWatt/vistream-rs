use serde::{Serialize, Deserialize};
use serde_repr::{Serialize_repr, Deserialize_repr};

#[derive(Debug, Default, Deserialize, Serialize, Clone, Copy)]
pub struct LocationData {
    pub x: f64,
    pub y: f64,
    pub z: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub yaw: Option<f64>,
    pub pitch: Option<f64>,
    pub roll: Option<f64>,
    pub id: Option<u32>,
}

#[derive(Default, Debug, Clone, Copy)]
pub struct LocationDataBuilder {
    x: Option<f64>,
    y: Option<f64>,
    z: Option<f64>,
    width: Option<f64>,
    height: Option<f64>,
    yaw: Option<f64>,
    pitch: Option<f64>,
    roll: Option<f64>,
    id: Option<u32>,
}

#[allow(dead_code)]
impl LocationDataBuilder {
    pub fn new() -> LocationDataBuilder {
        LocationDataBuilder::default()
    }

    pub fn x(&mut self, x: f64) -> &mut Self {
        self.x = Some(x);
        self
    }
    pub fn y(&mut self, y: f64) -> &mut Self {
        self.y = Some(y);
        self
    }
    pub fn z(&mut self, z: f64) -> &mut Self {
        self.z = Some(z);
        self
    }
    pub fn width(&mut self, width: f64) -> &mut Self {
        self.width = Some(width);
        self
    }
    pub fn height(&mut self, height: f64) -> &mut Self {
        self.height = Some(height);
        self
    }
    pub fn yaw(&mut self, yaw: f64) -> &mut Self {
        self.yaw = Some(yaw);
        self
    }
    pub fn pitch(&mut self, pitch: f64) -> &mut Self {
        self.pitch = Some(pitch);
        self
    }
    pub fn roll(&mut self, roll: f64) -> &mut Self {
        self.roll = Some(roll);
        self
    }
    pub fn id(&mut self, id: u32) -> &mut Self {
        self.id = Some(id);
        self
    }
    
    pub fn build(self) -> Result<LocationData, &'static str> {
        Ok(LocationData {
            x: self.x.ok_or("x coordinate required")?,
            y: self.y.ok_or("y coordinate required")?,
            z: self.z,
            width: self.width,
            height: self.height,
            yaw: self.yaw,
            pitch: self.pitch,
            roll: self.roll,
            id: self.id,
        })
    }
}

impl LocationData {

    pub fn builder() -> LocationDataBuilder {
        LocationDataBuilder::new()
    }

    pub fn two_d(x: f64, y: f64) -> LocationData {
        LocationData {
            x,
            y,
            ..Default::default()
        }
    }

    pub fn three_d(x: f64, y: f64, z: f64) -> LocationData {
        LocationData {
            x,
            y,
            z: Some(z),
            ..Default::default()
        }
    }
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
    pub framerate: f64,
}

// No format listed. Will always be jpg
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>
}
