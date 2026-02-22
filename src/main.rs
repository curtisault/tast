use std::path::PathBuf;

use clap::{Parser, Subcommand};

use tast::cli::commands;

#[derive(Parser)]
#[command(name = "tast", about = "TAST — Test Abstract Syntax Tree", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile test plans from .tast files
    Plan {
        /// Input .tast files
        files: Vec<PathBuf>,

        /// Output file (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Validate .tast files without compiling
    Validate {
        /// Input .tast files
        files: Vec<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Plan { files, output }) => {
            if files.is_empty() {
                eprintln!("error: no input files provided");
                std::process::exit(1);
            }
            match commands::run_plan(&files, output.as_ref()) {
                Ok(result) => print!("{result}"),
                Err(e) => {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Validate { files }) => {
            if files.is_empty() {
                eprintln!("error: no input files provided");
                std::process::exit(1);
            }
            match commands::run_validate(&files) {
                Ok(result) => println!("{result}"),
                Err(e) => {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
        }
        None => {
            // No subcommand — clap will show help via the derive
            Cli::parse_from(["tast", "--help"]);
        }
    }
}
