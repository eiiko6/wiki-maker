use anyhow::{Context, Result};
use pulldown_cmark::{Event, Parser, Tag};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::WikiConfig;

pub struct WikiGraph {
    pub nodes: HashMap<String, WikiConfig>,
    pub edges: HashMap<String, Vec<String>>,
}

impl WikiGraph {
    pub async fn new(docs_dir: PathBuf) -> Result<Self> {
        let mut nodes = HashMap::new();
        let mut raw_files = HashMap::new();

        // Scan for all TOML files
        let mut entries = tokio::fs::read_dir(&docs_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                let content = tokio::fs::read_to_string(&path).await?;
                let config: WikiConfig = toml::from_str(&content)
                    .with_context(|| format!("Failed to parse {:?}", path))?;

                let slug = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap()
                    .to_string();

                nodes.insert(slug.clone(), config.clone());
                raw_files.insert(slug, config);
            }
        }

        // Scan content for links
        let mut edges = HashMap::new();
        for (slug, config) in &raw_files {
            let mut page_links = Vec::new();

            if let Some(content_file) = &config.content_file {
                let md_path = docs_dir.join(content_file);
                if md_path.exists() {
                    let md_content = tokio::fs::read_to_string(&md_path).await?;
                    let links = extract_markdown_links(&md_content);

                    for link in links {
                        // Normalize link
                        let target_slug = normalize_link(&link);
                        // Only add if not a self-link
                        if target_slug != *slug {
                            page_links.push(target_slug);
                        }
                    }
                }
            }
            edges.insert(slug.clone(), page_links);
        }

        Ok(Self { nodes, edges })
    }

    pub fn print_dot(&self) {
        println!("digraph Wiki {{");
        println!("  graph [layout=neato, overlap=false, splines=true];");
        println!("  node [shape=box, style=\"filled,rounded\", fontname=\"Helvetica\"];");

        // Nodes
        for (slug, config) in &self.nodes {
            println!(
                "  \"{}\" [label=\"{}\", href=\"{}.html\"];",
                slug, config.title, slug
            );
        }

        println!("");

        // Edges
        for (source, targets) in &self.edges {
            for target in targets {
                if self.nodes.contains_key(target) {
                    println!("  \"{}\" -> \"{}\";", source, target);
                }
            }
        }

        println!("}}");
    }

    pub fn check_dead_links(&self) {
        let mut found_issues = false;

        for (source, targets) in &self.edges {
            for target in targets {
                if !self.nodes.contains_key(target) {
                    println!("\x1b[1m{}\x1b[0m -> ❌ \x1b[31m{}\x1b[0m", source, target);
                    found_issues = true;
                }
            }
        }

        if !found_issues {
            println!("✅ No broken links found!");
        }
    }
}

fn extract_markdown_links(content: &str) -> Vec<String> {
    let parser = Parser::new(content);
    let mut links = Vec::new();

    for event in parser {
        if let Event::Start(Tag::Link { dest_url, .. }) = event {
            let url = dest_url.to_string();
            // Filter out external links
            if !url.starts_with("http") && !url.starts_with("mailto:") && !url.starts_with('#') {
                links.push(url);
            }
        }
    }
    links
}

fn normalize_link(link: &str) -> String {
    let path = Path::new(link);
    // Remove extension
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or(link);
    // Remove leading ./
    stem.trim_start_matches("./").to_string()
}
