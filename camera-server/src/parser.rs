use clap::{Parser, Subcommand, Args, ValueEnum};

#[derive(Parser)]
#[command(version, about, long_about=None)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// List the available cameras
    List,
    /// Attempt to start the vistream server
    Launch(Launch),
    /// Create an alias for a specific camera
    Alias(Alias),
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum FourCC {
    #[value(alias = "RG24")]
    RG24,
    #[value(alias = "BG24")]
    BG24,
    #[value(alias = "RA24")]
    RA24,
    #[value(alias = "BA24")]
    BA24,
    #[value(alias = "YUYV")]
    YUYV,
    #[value(alias = "MJPG")]
    MJPG,
}
impl std::fmt::Display for FourCC {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}


#[derive(Args)]
#[command(arg_required_else_help = true)]
pub struct Launch {
    /// The name, id, or alias of the camera
    pub name: String,
    /// FourCC describing the output format
    #[arg(long, value_enum, default_value_t = FourCC::RG24, value_name = "FOURCC")]
    pub format: FourCC,

    /// Requested width of the output, in pixels. (not guaranteed to be respected)
    #[arg(long)]
    pub width: Option<usize>,
    /// Requested height of the output, in pixels. (not guaranteed to be respected)
    #[arg(long)]
    pub height: Option<usize>,
}

#[derive(Args)]
pub struct Alias {
    /// Name or ID of the camera. ID must be used if there are duplicate names.
    pub name: String,
    /// New name for the referenced camera. Associates with the ID
    pub alias: String,
}


