#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("camera has no most-recent framedata")]
    NoFrameData,
    #[error("camera is stopped")]
    NotStarted,
    #[error("existing camera's format is incompatible")]
    IncompatibleFormat,
    #[error("invalid frame data recieved")]
    FrameData,
    #[error("unknown error occurred")]
    Unknown,
    #[error("source was corrupted")]
    CorruptSource,
    #[error("camera connection timeout")]
    Timeout,

    #[error("server error: {0}")]
    Server(String),

    #[error(transparent)]
    IO(#[from] std::io::Error),

    #[error(transparent)]
    InEncoding(#[from] rmp_serde::decode::Error),

    #[cfg(feature = "ws")]
    #[error(transparent)]
    WebSocket(#[from] tungstenite::Error),
    #[cfg(feature = "ws")]
    #[error("something went wrong with the WebSocket handshake")]
    Handshake,
}

// #[cfg(feature = "ws")]
// pub enum WSError {
// 
// }

pub type Result<T> = std::result::Result<T, Error>;
