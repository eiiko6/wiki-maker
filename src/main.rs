use ax_models::{Page, WikiConfig};
use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::get,
};
use clap::{Parser, Subcommand};
use lazy_static::lazy_static;
use pulldown_cmark::{Options, Parser as MarkdownParser, html};
use std::sync::Arc;
use std::{io::Cursor, path::PathBuf};
use syntect::{highlighting::ThemeSet, parsing::SyntaxSet};
use tera::{Context, Tera};

mod codeblocks;
use codeblocks::*;

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

#[derive(Parser)]
#[command(author, version, about = "A simple wiki server/builder")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
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
    Build {
        #[arg(short, long)]
        path: PathBuf,
        #[arg(short, long)]
        no_navigation: bool,
        #[arg(short, long)]
        out_dir: Option<PathBuf>,
    },
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

    match cli.command {
        Commands::Serve {
            path,
            port,
            host,
            no_navigation,
        } => {
            let abs_path = std::fs::canonicalize(&path)?;
            let shared_state = Arc::new(AppState {
                docs_dir: abs_path,
                no_navigation,
            });
            let app = Router::new()
                .route("/", get(render_summary_handler))
                .route("/{page}", get(render_page_handler))
                .route("/style.css", get(serve_css))
                // Serve images relative to the docs directory
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
            path,
            no_navigation,
            out_dir,
        } => {
            let abs_path = std::fs::canonicalize(&path)?;
            let output_path = out_dir.unwrap_or_else(|| abs_path.clone());
            tokio::fs::create_dir_all(&output_path).await?;
            run_build(abs_path, output_path, no_navigation).await?;
        }
    }
    Ok(())
}

async fn get_summary_data(docs_dir: &PathBuf) -> Vec<Page> {
    let mut pages = Vec::new();
    if let Ok(mut entries) = tokio::fs::read_dir(docs_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            // We now look for TOML files as the entry points
            if path.extension().and_then(|s| s.to_str()) != Some("toml") {
                continue;
            }

            let filename = entry.file_name();
            let filename_str = filename.to_str().unwrap_or("");

            // Read TOML to get the title
            let title = if let Ok(content) = tokio::fs::read_to_string(&path).await {
                if let Ok(config) = toml::from_str::<WikiConfig>(&content) {
                    config.title
                } else {
                    filename_str.to_string()
                }
            } else {
                filename_str.to_string()
            };

            // TODO:
            let datetime = "".to_string();

            pages.push(Page {
                filename: filename_str.to_string(), // Keep .toml extension here for now
                title,
                datetime,
            });
        }
    }
    pages.sort_by(|a, b| a.title.cmp(&b.title));
    pages
}

async fn render_wiki_page(
    filename: &str,
    docs_dir: &PathBuf,
    no_navigation: bool,
    is_static: bool,
) -> Result<String, String> {
    let toml_path = docs_dir.join(filename);
    let toml_content = tokio::fs::read_to_string(&toml_path)
        .await
        .map_err(|_| "Page configuration not found".to_string())?;

    let config: WikiConfig =
        toml::from_str(&toml_content).map_err(|e| format!("Invalid TOML configuration: {}", e))?;

    let markdown_content = if let Some(md_file) = &config.content_file {
        let md_path = docs_dir.join(md_file);
        tokio::fs::read_to_string(&md_path)
            .await
            .unwrap_or_else(|_| {
                "# Content missing\nThe linked markdown file could not be found.".to_string()
            })
    } else {
        String::new()
    };

    // Render Markdown
    let mut options = Options::empty();
    options.insert(
        Options::ENABLE_TABLES
            | Options::ENABLE_FOOTNOTES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS,
    );

    let parser = MarkdownParser::new_ext(&markdown_content, options);
    let renderer = CodeblockRenderer::new(parser);
    let mut html_output = String::new();
    html::push_html(&mut html_output, renderer);

    let (mut prev, mut next) = if no_navigation {
        (None, None)
    } else {
        get_nav_links(docs_dir, filename)
    };

    if is_static {
        prev = prev.map(|s| {
            if s == "." {
                "index.html".to_string()
            } else {
                s.replace(".toml", ".html")
            }
        });
        next = next.map(|s| s.replace(".toml", ".html"));
    }

    let infobox_list: Vec<InfoboxItem> = match config.infobox {
        Some(map) => map
            .into_iter()
            .map(|(k, v)| InfoboxItem { key: k, value: v })
            .collect(),
        None => Vec::new(),
    };

    let mut context = Context::new();
    context.insert("title", &config.title);
    context.insert("content", &html_output);
    context.insert("infobox", &infobox_list); // Pass the ordered list, not the map
    context.insert("main_image", &config.image);
    context.insert("prev_page", &prev);
    context.insert("next_page", &next);
    context.insert("no_navigation", &no_navigation);
    context.insert("is_static", &is_static);

    TEMPLATES
        .render("page.html", &context)
        .map_err(|e| format!("Template Error: {}", e))
}

async fn run_build(docs_dir: PathBuf, out_dir: PathBuf, no_navigation: bool) -> anyhow::Result<()> {
    tracing::info!("Building static site to: {:?}", out_dir);

    // Build summary
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

    // Build css
    let css = TEMPLATES.render("style.css", &Context::new())?;
    tokio::fs::write(out_dir.join("style.css"), css).await?;

    // Copy assets (images, etc)
    // NOTE: In a real app you'd recursively copy everything not .md/.toml
    // For now we just copy files that look like images if they are in root
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

    // Build pages
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

async fn render_summary_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    if state.no_navigation {
        return (StatusCode::NOT_FOUND, "Disabled").into_response();
    }
    let pages = get_summary_data(&state.docs_dir).await;
    let mut context = Context::new();
    context.insert("title", "Wiki Index");
    context.insert("files", &pages);
    context.insert("is_static", &false);

    match TEMPLATES.render("home.html", &context) {
        Ok(rendered) => Html(rendered).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn render_page_handler(
    State(state): State<Arc<AppState>>,
    Path(page): Path<String>,
) -> impl IntoResponse {
    let filename = if page.ends_with(".toml") {
        page
    } else {
        format!("{}.toml", page)
    };

    match render_wiki_page(&filename, &state.docs_dir, state.no_navigation, false).await {
        Ok(html) => Html(html).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, format!("<h1>404</h1><p>{}</p>", e)).into_response(),
    }
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

mod ax_models {
    use indexmap::IndexMap;
    use serde::{Deserialize, Serialize}; // Use IndexMap instead of BTreeMap

    #[derive(Deserialize, Serialize, Clone)]
    pub struct Page {
        pub filename: String,
        pub title: String,
        pub datetime: String,
    }

    #[derive(Deserialize, Serialize, Clone)]
    pub struct WikiConfig {
        pub title: String,
        pub image: Option<String>,
        // IndexMap preserves the order from the file
        pub infobox: Option<IndexMap<String, String>>,
        pub content_file: Option<String>,
    }
}

// Helper struct for the template
#[derive(serde::Serialize)]
struct InfoboxItem {
    key: String,
    value: String,
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
