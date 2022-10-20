use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Clone, Parser)]
#[command(author, version, about, long_about = None)]
pub(crate) struct Config {
    /// Image directory
    #[arg(short, long, value_name = "IMAGE_DIR")]
    pub image_dir: PathBuf,

    /// EPD height
    #[arg(short = 'H', long)]
    pub epd_height: u32,

    /// EPD width
    #[arg(short = 'W', long)]
    pub epd_width: u32,
}
