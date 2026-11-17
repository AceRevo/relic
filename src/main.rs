mod cli;
mod engine;
mod hardware;
mod tui;

use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();

    // Initialize structured logging
    tracing_subscriber::fmt()
        .with_env_filter(if args.verbose {
            "relic=debug,info"
        } else {
            "relic=info,warn,error"
        })
        .init();

    match args.command {
        Some(Commands::Capture {
            source,
            output,
            compression_level,
            block_size_mb,
        }) => {
            println!(
                "[*] Initiating block capture of {:?} -> {:?}",
                source, output
            );
            engine::stream::capture_volume(&source, &output, compression_level, block_size_mb).await?;
        }
        Some(Commands::Restore {
            image,
            target,
            force,
        }) => {
            println!(
                "[*] Initiating bare-metal restoration of {:?} onto target {:?}",
                image, target
            );
            if !force {
                eprintln!("[!] WARNING: Target device will be overwritten. Use --force to execute.");
                return Ok(());
            }
            engine::stream::restore_volume(&image, &target).await?;
        }
        Some(Commands::Inspect { detailed }) => {
            hardware::detector::scan_devices(detailed)?;
        }
        Some(Commands::Verify { image }) => {
            engine::verifier::verify_image(&image).await?;
        }
        None => {
            // Launch the futuristic Interactive Terminal UI
            tui::interface::run_interactive_tui().await?;
        }
    }

    Ok(())
}
