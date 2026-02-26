use std::path::PathBuf;

use clap::{Parser, Subcommand};

use tast::cli::commands::{self, PlanOptions, RunOptions};

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

        /// Output format (yaml, markdown, junit)
        #[arg(short = 'F', long, default_value = "yaml")]
        format: String,

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

    /// Execute test plans against a project
    Run {
        /// .tast files to run
        files: Vec<PathBuf>,

        /// Test backend to use (default: auto-detect)
        #[arg(long, short = 'b')]
        backend: Option<String>,

        /// Output format for results (yaml, json, junit)
        #[arg(long, short = 'f', default_value = "yaml")]
        format: String,

        /// Write results to a file instead of stdout
        #[arg(long, short = 'o')]
        output: Option<PathBuf>,

        /// Tag filter expression
        #[arg(long)]
        filter: Option<String>,

        /// Maximum parallel steps
        #[arg(long, short = 'p', default_value = "1")]
        parallel: usize,

        /// Per-step timeout in seconds
        #[arg(long, short = 't', default_value = "60")]
        timeout: u64,

        /// Stop on first failure
        #[arg(long)]
        fail_fast: bool,

        /// Keep generated harness files after run (for debugging)
        #[arg(long)]
        keep_harness: bool,

        /// Graph traversal strategy
        #[arg(long, short = 's', default_value = "topological")]
        strategy: String,
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
            format,
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
                format,
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
        Some(Commands::Run {
            files,
            backend,
            format,
            output,
            filter,
            parallel,
            timeout,
            fail_fast,
            keep_harness,
            strategy,
        }) => {
            if files.is_empty() {
                eprintln!("error: no input files provided");
                std::process::exit(1);
            }
            let options = RunOptions {
                files,
                backend,
                format,
                output,
                filter,
                parallel,
                timeout,
                fail_fast,
                keep_harness,
                strategy,
            };
            match commands::run_run(options) {
                Ok(success) => {
                    if !success {
                        std::process::exit(1);
                    }
                }
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
