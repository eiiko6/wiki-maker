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
        /// Disable the navigation page
        #[arg(short, long)]
        no_navigation: bool,
        /// Network port to use
        #[arg(short = 'P', long, default_value = "8090")]
        port: u16,
        /// Whether to use 0.0.0.0 to serve on the network
        #[arg(short = 'H', long)]
        host: bool,
    },
    /// Build the static site
    Build {
        #[arg(short, long)]
        no_navigation: bool,
        #[arg(short, long, default_value = "dist")]
        out_dir: PathBuf,
    },
    /// Output a DOT graph of the wiki connections
    Graph {},
    /// List broken links
    Todo {
        /// Invert the display: "Sources -> Target" instead of "Target <- Sources"
        #[arg(short, long)]
        reverse: bool,
    },
    /// Manage wiki entries
    Entry {
        #[command(subcommand)]
        cmd: EntryCommands,
    },
    /// Manage the changelog
    Changelog {
        #[command(subcommand)]
        cmd: ChangelogCommands,
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

#[derive(Subcommand)]
pub enum ChangelogCommands {
    /// View the current changelog
    View,
    /// Generate or update the changelog from git history
    GenFromGit {
        /// Path to the directory containing `.git/` (defaults to current dir or wiki dir).
        /// Note that git will look in parent directories until it finds a `.git/`
        #[arg(long)]
        git_path: Option<PathBuf>,
    },
}
