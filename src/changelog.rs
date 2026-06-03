use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::fs;

use crate::cli::ChangelogCommands;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChangelogEntry {
    pub date: String,
    pub message: String,
    pub author: String,
    pub hash: String,
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ChangelogConfig {
    pub entries: Vec<ChangelogEntry>,
}

pub async fn handle(cmd: ChangelogCommands, root_dir: PathBuf) -> Result<()> {
    let changelog_path = root_dir.join("_changelog.toml");

    match cmd {
        ChangelogCommands::View => {
            if !changelog_path.exists() {
                println!("No changelog found at {:?}", changelog_path);
                return Ok(());
            }
            let content = fs::read_to_string(&changelog_path).await?;
            let config: ChangelogConfig = toml::from_str(&content)?;

            println!("Changelog ({} entries):", config.entries.len());
            for entry in config.entries {
                println!("- [{}] {} ({})", entry.date, entry.message, entry.author);
                if !entry.files.is_empty() {
                    println!("  Files: {:?}\n", entry.files);
                }
            }
        }
        ChangelogCommands::GenFromGit { git_path } => {
            let git_dir = git_path.unwrap_or_else(|| root_dir.clone());
            update_from_git(&changelog_path, &git_dir, &root_dir).await?;
        }
    }
    Ok(())
}

async fn update_from_git(changelog_path: &Path, git_dir: &Path, wiki_root: &Path) -> Result<()> {
    let output_root = Command::new("git")
        .arg("-C")
        .arg(git_dir)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("Failed to find git root")?;

    let repo_root_str = String::from_utf8(output_root.stdout)?.trim().to_string();
    let repo_root = PathBuf::from(&repo_root_str);

    let prefix_path = wiki_root.strip_prefix(&repo_root).unwrap_or(Path::new(""));

    let mut prefix = prefix_path.to_string_lossy().to_string().replace('\\', "/");
    if !prefix.is_empty() && !prefix.ends_with('/') {
        prefix.push('/');
    }

    // Load existing
    let mut config = if changelog_path.exists() {
        let content = fs::read_to_string(changelog_path).await?;
        toml::from_str(&content).unwrap_or_default()
    } else {
        ChangelogConfig::default()
    };

    let existing_hashes: HashSet<String> = config.entries.iter().map(|e| e.hash.clone()).collect();

    let output = Command::new("git")
        .arg("-C")
        .arg(git_dir)
        .args([
            "log",
            "--name-only",
            "--pretty=format:---ENTRY---|%H|%ad|%an|%s",
            "--date=format:%Y-%m-%d %H:%M:%S",
            "--",
            ".",
        ])
        .output()
        .context("Failed to execute git command")?;

    if !output.status.success() {
        return Err(anyhow!(
            "Git command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8(output.stdout)?;
    let mut new_entries = Vec::new();
    let mut current_entry: Option<ChangelogEntry> = None;

    for line in stdout.lines() {
        if line.starts_with("---ENTRY---|") {
            // Push previous
            if let Some(entry) = current_entry.take()
                && !entry.files.is_empty()
                && !existing_hashes.contains(&entry.hash)
            {
                new_entries.push(entry);
            }

            // Parse new
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 5 {
                let hash = parts[1].to_string();
                let date = parts[2].to_string();
                let author = parts[3].to_string();
                let message = parts[4..].join("|");

                current_entry = Some(ChangelogEntry {
                    hash,
                    date,
                    author,
                    message,
                    files: Vec::new(),
                });
            }
        } else if !line.trim().is_empty() {
            let raw_path = line.trim();

            // Check if file belongs to the wiki directory
            if (prefix.is_empty() || raw_path.starts_with(&prefix))
                && let Some(entry) = current_entry.as_mut()
            {
                // Strip the prefix to get the relative path inside the wiki
                let clean_path = if prefix.is_empty() {
                    raw_path.to_string()
                } else {
                    raw_path
                        .strip_prefix(&prefix)
                        .unwrap_or(raw_path)
                        .to_string()
                };

                if clean_path != "changelog.toml" {
                    entry.files.push(clean_path);
                }
            }
        }
    }

    // Push last
    if let Some(entry) = current_entry
        && !entry.files.is_empty()
        && !existing_hashes.contains(&entry.hash)
    {
        new_entries.push(entry);
    }

    if new_entries.is_empty() {
        println!("No new commits touching the wiki directory found.");
        return Ok(());
    }

    println!("Found {} new commits.", new_entries.len());

    config.entries.extend(new_entries);
    config.entries.sort_by(|a, b| b.date.cmp(&a.date));

    let mut toml_str =
        String::from("# This file was generated by `wiki-maker changelog gen-from-git`\n\n");
    toml_str.push_str(&toml::to_string_pretty(&config)?);
    fs::write(changelog_path, toml_str).await?;
    println!("Updated {:?}", changelog_path);

    Ok(())
}

pub async fn load_changelog(root_dir: &Path) -> Vec<ChangelogEntry> {
    let path = root_dir.join("_changelog.toml");
    if path.exists()
        && let Ok(content) = fs::read_to_string(path).await
        && let Ok(config) = toml::from_str::<ChangelogConfig>(&content)
    {
        return config.entries;
    }
    Vec::new()
}
