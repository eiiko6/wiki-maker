use anyhow::{Context, Result, anyhow};
use colored::Colorize;
use std::path::PathBuf;
use tokio::fs;

use crate::WikiConfig;
use crate::cli::EntryCommands;

pub async fn handle(command: EntryCommands, root_dir: PathBuf) -> Result<()> {
    match command {
        EntryCommands::List => list_entries(&root_dir).await,
        EntryCommands::New { name } => create_entry(&root_dir, &name).await,
        EntryCommands::Remove { name } => remove_entry(&root_dir, &name).await,
        EntryCommands::Inspect { name } => inspect_entry(&root_dir, &name).await,
    }
}

async fn list_entries(root: &PathBuf) -> Result<()> {
    let mut entries = fs::read_dir(root).await?;
    let mut rows = Vec::new();

    // Scan for .toml files
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("toml") {
            let name = path.file_stem().unwrap().to_string_lossy().to_string();

            // Read configuration to check for linked files
            let content = fs::read_to_string(&path).await.unwrap_or_default();
            let (md_status, img_status) = if let Ok(config) = toml::from_str::<WikiConfig>(&content)
            {
                let md = check_file_status(root, config.content_file);
                let img = check_file_status(root, config.image);
                (md, img)
            } else {
                ("Invalid Config".to_string(), "Unknown".to_string())
            };

            rows.push((name, md_status, img_status));
        }
    }

    rows.sort_by(|a, b| a.0.cmp(&b.0));

    // Print Table
    println!(
        "{:<25} {:<25} {:<25}",
        "Entry Name", "Markdown File", "Image"
    );

    for (name, md, img) in rows {
        println!("{:<25} {:<25} {:<25}", name, md, img);
    }

    Ok(())
}

fn check_file_status(root: &PathBuf, filename: Option<String>) -> String {
    match filename {
        Some(f) => {
            if root.join(&f).exists() {
                f.green().to_string()
            } else {
                format!("{} (Missing)", f.red())
            }
        }
        None => "None".to_string(),
    }
}

async fn create_entry(root: &PathBuf, title: &str) -> Result<()> {
    let slug: String = title
        .trim()
        .to_lowercase()
        .replace([' ', '_'], "-")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect();

    if slug.is_empty() {
        return Err(anyhow!("Resulting entry name is empty"));
    }

    let toml_filename = format!("{}.toml", slug);
    let md_filename = format!("{}.md", slug);
    let img_filename = format!("{}.png", slug);

    let toml_path = root.join(&toml_filename);
    let md_path = root.join(&md_filename);

    if toml_path.exists() {
        return Err(anyhow!("Entry '{}' already exists", slug));
    }

    // Minimal default content
    let toml_content = format!(
        r#"title = "{}"
image = "{}"
content_file = "{}"

[infobox]
"Type" = "Draft"
"Status" = "WIP"
"Created" = "2026"
"Category" = "General""#,
        title, img_filename, md_filename
    );

    fs::write(&toml_path, toml_content)
        .await
        .context("Failed to write TOML")?;

    // Markdown Placeholder
    let md_content = format!("Auto-generated markdown file for \"{}\"...", title);
    fs::write(&md_path, md_content)
        .await
        .context("Failed to write Markdown")?;

    println!("Created entry '{}'", title);
    println!("  - TOML: {:?}", toml_path);
    println!("  - Markdown: {:?}", md_path);
    println!("  - (Expected image: {:?})", img_filename);

    Ok(())
}

async fn remove_entry(root: &PathBuf, name: &str) -> Result<()> {
    let toml_path = root.join(format!("{}.toml", name));

    if !toml_path.exists() {
        return Err(anyhow!(
            "Entry '{}' not found (looked at {:?})",
            name,
            toml_path
        ));
    }

    let content = fs::read_to_string(&toml_path).await?;
    let config: WikiConfig = toml::from_str(&content).context("Failed to parse TOML")?;

    // Try to remove linked files
    if let Some(md_file) = config.content_file {
        let p = root.join(&md_file);
        if p.exists() {
            fs::remove_file(&p).await?;
            println!("Removed: {}", md_file);
        }
    }

    if let Some(img_file) = config.image {
        let p = root.join(&img_file);
        if p.exists() {
            fs::remove_file(&p).await?;
            println!("Removed: {}", img_file);
        }
    }

    fs::remove_file(&toml_path).await?;
    println!("Removed: {}.toml", name);

    Ok(())
}

async fn inspect_entry(root: &PathBuf, name: &str) -> Result<()> {
    let toml_path = root.join(format!("{}.toml", name));
    if !toml_path.exists() {
        return Err(anyhow!("Entry '{}' not found", name));
    }

    let content = fs::read_to_string(&toml_path).await?;
    println!("{}", content);
    Ok(())
}
