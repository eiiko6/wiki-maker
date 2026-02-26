use axum::{
    Router,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use clap::Parser;
use lazy_static::lazy_static;
use std::sync::Arc;
use std::{io::Cursor, path::PathBuf};
use syntect::{highlighting::ThemeSet, parsing::SyntaxSet};
use tera::{Context, Tera};

mod analysis;
mod cli;
mod entry;
mod rendering;

use crate::{
    cli::{Cli, Commands},
    rendering::{render_page_handler, render_summary_handler, render_wiki_page},
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

lazy_static! {
    pub static ref TEMPLATES: Tera = {
        let mut tera = Tera::default();
        tera.add_raw_templates(vec![
            ("_base.html", include_str!("../templates/_base.html")),
            ("home.html", include_str!("../templates/home.html")),
            ("page.html", include_str!("../templates/page.html")),
            ("style.css", include_str!("../templates/style.css")),
        ])
        .unwrap();
        tera
    };
    pub static ref SYNTAX_SET: SyntaxSet = SyntaxSet::load_defaults_newlines();
    pub static ref THEME_SET: ThemeSet = {
        let mut set = ThemeSet::load_defaults();
        let theme_bytes = include_bytes!(env!("THEME_FILE_PATH"));
        let mut cursor = Cursor::new(theme_bytes);
        match syntect::highlighting::ThemeSet::load_from_reader(&mut cursor) {
            Ok(theme) => {
                set.themes.insert("Catppuccin Macchiato".to_string(), theme);
            }
            Err(e) => {
                tracing::error!("Failed to load embedded theme: {}", e);
            }
        }
        set
    };
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Page {
    pub filename: String,
    pub title: String,
    pub datetime: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct WikiConfig {
    pub title: String,
    pub image: Option<String>,
    pub infobox: Option<IndexMap<String, String>>,
    pub content_file: Option<String>,
}

struct AppState {
    docs_dir: PathBuf,
    no_navigation: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    lazy_static::initialize(&TEMPLATES);
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let cli = Cli::parse();

    // Resolve the global path argument once
    let abs_path = std::fs::canonicalize(&cli.path)?;

    match cli.command {
        Commands::Serve {
            port,
            host,
            no_navigation,
        } => {
            let shared_state = Arc::new(AppState {
                docs_dir: abs_path,
                no_navigation,
            });
            let app = Router::new()
                .route("/", get(render_summary_handler))
                .route("/{page}", get(render_page_handler))
                .route("/style.css", get(serve_css))
                .nest_service(
                    "/images",
                    tower_http::services::ServeDir::new(&shared_state.docs_dir.join("images")),
                )
                .nest_service(
                    "/assets",
                    tower_http::services::ServeDir::new(&shared_state.docs_dir),
                )
                .with_state(shared_state);

            let addr = if host {
                format!("0.0.0.0:{}", port)
            } else {
                format!("127.0.0.1:{}", port)
            };
            let listener = tokio::net::TcpListener::bind(&addr).await?;
            tracing::info!("Listening on http://{}", addr);
            axum::serve(listener, app).await?;
        }
        Commands::Build {
            no_navigation,
            out_dir,
        } => {
            let output_path = out_dir;
            tokio::fs::create_dir_all(&output_path).await?;
            run_build(abs_path, output_path, no_navigation).await?;
        }
        Commands::Graph {} => {
            let graph = analysis::WikiGraph::new(abs_path).await?;
            graph.print_dot();
        }
        Commands::Todo { reverse } => {
            let graph = analysis::WikiGraph::new(abs_path).await?;
            graph.check_dead_links(reverse);
        }
        Commands::Entry { cmd } => {
            entry::handle(cmd, abs_path).await?;
        }
    }
    Ok(())
}

async fn get_summary_data(docs_dir: &PathBuf) -> Vec<Page> {
    let mut pages = Vec::new();
    if let Ok(mut entries) = tokio::fs::read_dir(docs_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("toml") {
                continue;
            }

            let filename = entry.file_name();
            let filename_str = filename.to_str().unwrap_or("");

            let title = if let Ok(content) = tokio::fs::read_to_string(&path).await {
                if let Ok(config) = toml::from_str::<WikiConfig>(&content) {
                    config.title
                } else {
                    filename_str.to_string()
                }
            } else {
                filename_str.to_string()
            };

            pages.push(Page {
                filename: filename_str.to_string(),
                title,
                datetime: "".to_string(),
            });
        }
    }
    pages.sort_by(|a, b| a.title.cmp(&b.title));
    pages
}

async fn run_build(docs_dir: PathBuf, out_dir: PathBuf, no_navigation: bool) -> anyhow::Result<()> {
    tracing::info!("Building static site to: {:?}", out_dir);

    if !no_navigation {
        let pages = get_summary_data(&docs_dir).await;
        let static_pages: Vec<Page> = pages
            .into_iter()
            .map(|mut p| {
                p.filename = p.filename.replace(".toml", ".html");
                p
            })
            .collect();

        let mut context = Context::new();
        context.insert("title", "Wiki Index");
        context.insert("files", &static_pages);
        context.insert("is_static", &true);

        let rendered = TEMPLATES.render("home.html", &context)?;
        tokio::fs::write(out_dir.join("index.html"), rendered).await?;
    }

    let css = TEMPLATES.render("style.css", &Context::new())?;
    tokio::fs::write(out_dir.join("style.css"), css).await?;

    let mut entries = tokio::fs::read_dir(&docs_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            if ["png", "jpg", "jpeg", "gif", "webp"].contains(&ext) {
                let dest = out_dir.join(path.file_name().unwrap());
                tokio::fs::copy(path, dest).await?;
            }
        }
    }

    let out_images = out_dir.join("images");
    tokio::fs::create_dir_all(&out_images).await?;

    let src_images = docs_dir.join("images");
    if src_images.exists() {
        let mut entries = tokio::fs::read_dir(&src_images).await?;
        while let Some(entry) = entries.next_entry().await? {
            tracing::info!("Copied {}", entry.file_name().display(),);
            let path = entry.path();
            if path.is_file() {
                let dest = out_images.join(path.file_name().unwrap());
                tokio::fs::copy(path, dest).await?;
            }
        }
    }

    let mut entries = tokio::fs::read_dir(&docs_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("toml") {
            let filename = entry.file_name().to_str().unwrap().to_string();

            match render_wiki_page(&filename, &docs_dir, no_navigation, true).await {
                Ok(rendered) => {
                    let out_file = out_dir.join(filename.replace(".toml", ".html"));
                    tokio::fs::write(out_file, rendered).await?;
                    tracing::info!("Generated {}", filename);
                }
                Err(e) => tracing::error!("Failed to generate {}: {}", filename, e),
            }
        }
    }

    tracing::info!("Build complete!");
    Ok(())
}

async fn serve_css() -> impl IntoResponse {
    match TEMPLATES.render("style.css", &Context::new()) {
        Ok(css) => Response::builder()
            .header("content-type", "text/css")
            .body(css.into())
            .unwrap(),
        Err(_) => (StatusCode::NOT_FOUND, "CSS not found").into_response(),
    }
}

fn get_nav_links(dir: &PathBuf, current_file: &str) -> (Option<String>, Option<String>) {
    let mut files: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()? == "toml" {
                Some(path.file_name()?.to_str()?.to_string())
            } else {
                None
            }
        })
        .collect();

    files.sort();
    let pos = files.iter().position(|f| f == current_file);
    match pos {
        Some(i) => {
            let prev = if i == 0 {
                Some(".".to_string())
            } else {
                files.get(i - 1).cloned()
            };
            let next = files.get(i + 1).cloned();
            (prev, next)
        }
        None => (None, None),
    }
}
