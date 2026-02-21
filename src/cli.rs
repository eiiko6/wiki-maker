use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about = "A simple wiki server/builder")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Serve the wiki locally
    Serve {
        #[arg(short, long)]
        path: PathBuf,
        #[arg(short, long)]
        no_navigation: bool,
        #[arg(short = 'P', long, default_value = "8090")]
        port: u16,
        #[arg(short = 'H', long)]
        host: bool,
    },
    /// Build the static site
    Build {
        #[arg(short, long)]
        path: PathBuf,
        #[arg(short, long)]
        no_navigation: bool,
        #[arg(short, long)]
        out_dir: Option<PathBuf>,
    },
    /// Output a DOT graph of the wiki connections
    Graph {
        #[arg(short, long)]
        path: PathBuf,
    },
    /// List broken links
    Todo {
        #[arg(short, long)]
        path: PathBuf,
    },
}
