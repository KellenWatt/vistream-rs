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
    List(List),
    /// Attempt to start the vistream server
    Launch(Launch),
    /// Create an alias for a specific camera
    #[command(subcommand)]
    Alias(Alias),
    /// Attempt to resolve a name into a full camera id
    Resolve(Resolve),
    /// List the status of a given camera.
    Check(Check),
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
pub struct List {
    #[arg(long)]
    pub piped: bool,
}


#[derive(Args)]
#[command(arg_required_else_help = true)]
pub struct Launch {
    /// The name, id, or alias of the camera
    pub name: String,

    /// FourCC describing the output format
    #[arg(long, value_enum, default_value_t = FourCC::RG24, value_name = "FOURCC")]
    pub format: FourCC,

    #[arg(long, alias = "buffers", value_name = "COUNT", default_value_t = 1)]
    pub buffer_count: u32,
    // /// Requested maximum framerate. Default: unbounded
    // #[arg(long, alias = "fps", value_name = "FPS")]
    // pub framerate: Option<f64>,
    /// Requested width of the output, in pixels. (not guaranteed to be respected)
    #[arg(long)]
    pub width: Option<u32>,
    /// Requested height of the output, in pixels. (not guaranteed to be respected)
    #[arg(long)]
    pub height: Option<u32>,
    
    /// Does not try to launch a new server if camera is already acquired.
    #[arg(long, name = "allow-fail")]
    pub allow_fail: bool,
}

// #[derive(Args)]
// pub struct Alias {
//     /// Name or ID of the camera. ID must be used if there are duplicate names.
//     pub name: String,
//     /// New name for the referenced camera. Associates with the ID
//     pub alias: String,
// }

#[derive(Subcommand)]
pub enum Alias {
    /// List aliases
    List,
    /// Remove an alias
    Remove(RemoveAlias),
    /// Create an alias
    Create(CreateAlias),
}

#[derive(Args)]
pub struct RemoveAlias {
    /// Alias to be removed
    pub alias: String,
}

#[derive(Args)]
pub struct CreateAlias {
    /// Update an alias if it already exists (fails by default)
    #[arg(long, short)]
    pub update: bool,
    /// Name of alias to create. Must be unique
    pub alias: String,
    /// Name or ID the alias refers to. Doesn't need to be valid or unique
    pub name: String,
}


#[derive(Args)]
pub struct Resolve {
    /// The name to be resolved
    pub name: String,
}

#[derive(Args)]
pub struct Check {
    /// The name or alias of a camera to be checked
    pub name: String,
    /// Silences any output, using only the return code
    #[arg(long, short)]
    pub quiet: bool,
}
