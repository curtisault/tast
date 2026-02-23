use std::path::PathBuf;

use clap::{Parser, Subcommand};

use tast::cli::commands::{self, PlanOptions};

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

        /// Traversal strategy (topological, dfs, bfs)
        #[arg(short, long, default_value = "topological")]
        strategy: String,

        /// Filter nodes by tag predicate
        #[arg(short, long)]
        filter: Option<String>,

        /// Start node for path query
        #[arg(long)]
        from: Option<String>,

        /// End node for path query
        #[arg(long)]
        to: Option<String>,
    },

    /// Validate .tast files without compiling
    Validate {
        /// Input .tast files
        files: Vec<PathBuf>,
    },

    /// List nodes, edges, or tags from .tast files
    List {
        /// What to list: nodes, edges, or tags
        what: String,

        /// Input .tast files
        files: Vec<PathBuf>,
    },

    /// Visualize the test graph (DOT/Mermaid output)
    Visualize {
        /// Input .tast files
        files: Vec<PathBuf>,

        /// Output format (dot, mermaid)
        #[arg(short = 'F', long, default_value = "dot")]
        format: String,

        /// Output file (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Plan {
            files,
            output,
            strategy,
            filter,
            from,
            to,
        }) => {
            if files.is_empty() {
                eprintln!("error: no input files provided");
                std::process::exit(1);
            }
            let options = PlanOptions {
                output,
                strategy,
                filter,
                from,
                to,
            };
            match commands::run_plan(&files, &options) {
                Ok(result) => print!("{result}"),
                Err(e) => {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Visualize {
            files,
            format,
            output,
        }) => {
            if files.is_empty() {
                eprintln!("error: no input files provided");
                std::process::exit(1);
            }
            match commands::run_visualize(&files, &format, output.as_ref()) {
                Ok(result) => print!("{result}"),
                Err(e) => {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::List { what, files }) => {
            if files.is_empty() {
                eprintln!("error: no input files provided");
                std::process::exit(1);
            }
            match commands::run_list(&what, &files) {
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
