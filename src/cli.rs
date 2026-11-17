use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "relic",
    version,
    about = "Autonomous Bare-Metal Disaster Recovery and System Reanimation Engine",
    long_about = "RELIC is an ultra-fast bare-metal system recovery utility designed to take over compromised hosts and restore disk images with sub-minute latency using hardware-accelerated block streaming."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Run in standalone interactive TUI mode (default when no command is passed)
    #[arg(short, long, default_value_t = true)]
    pub interactive: bool,

    /// Verbose logging output
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Create a high-speed compressed snapshot (engram) of a target disk
    Capture {
        /// Target disk device path (e.g., /dev/nvme0n1 or \\.\PhysicalDrive0)
        #[arg(short, long)]
        source: PathBuf,

        /// Destination path for the compressed engram image
        #[arg(short, long)]
        output: PathBuf,

        /// Compression level (1-19, default: 3 for maximum throughput)
        #[arg(short, long, default_value_t = 3)]
        compression_level: i32,

        /// Block size in megabytes (default: 4MB)
        #[arg(short, long, default_value_t = 4)]
        block_size_mb: usize,
    },

    /// Restore a disk image to a physical target volume
    Restore {
        /// Path to the engram image container
        #[arg(short, long)]
        image: PathBuf,

        /// Target disk device path to overwrite
        #[arg(short, long)]
        target: PathBuf,

        /// Skip interactive confirmation (CAUTION: destroys existing target data)
        #[arg(long, default_value_t = false)]
        force: bool,
    },

    /// Scan and inspect all attached block storage devices and partitions
    Inspect {
        /// Show detailed SMART health and sector layouts
        #[arg(short, long, default_value_t = false)]
        detailed: bool,
    },

    /// Verify the BLAKE3 integrity checksums of an existing engram container
    Verify {
        /// Path to the engram image container
        #[arg(short, long)]
        image: PathBuf,
    },
}
