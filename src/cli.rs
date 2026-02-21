use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about = "A simple wiki server/builder")]
pub struct Cli {
    #[arg(short, long, global = true, default_value = ".")]
    pub path: PathBuf,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Serve the wiki locally
    Serve {
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
        no_navigation: bool,
        #[arg(short, long)]
        out_dir: Option<PathBuf>,
    },
    /// Output a DOT graph of the wiki connections
    Graph {},
    /// List broken links
    Todo {},
    /// Manage wiki entries
    Entry {
        #[command(subcommand)]
        cmd: EntryCommands,
    },
}

#[derive(Subcommand)]
pub enum EntryCommands {
    /// List all existing entries
    List,
    /// Create a new entry
    New {
        /// The title of the new entry (e.g. "The Great Bernardo")
        name: String,
    },
    /// Remove an entry by its normalized name
    Remove {
        /// The normalized name of the toml file (e.g. "the-great-bernardo")
        name: String,
    },
    /// Inspect the TOML config of an entry
    Inspect {
        /// The normalized name of the toml file (e.g. "the-great-bernardo")
        name: String,
    },
}
