use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse},
};
use pulldown_cmark::{Options, Parser as MarkdownParser, html};
use std::path::PathBuf;
use std::sync::Arc;
use tera::Context;

use crate::{
    AppState, TEMPLATES, WikiConfig, codeblocks::CodeblockRenderer, get_nav_links, get_summary_data,
};

#[derive(serde::Serialize)]
struct InfoboxItem {
    key: String,
    value: String,
}

pub async fn render_wiki_page(
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
    context.insert("infobox", &infobox_list);
    context.insert("main_image", &config.image);
    context.insert("prev_page", &prev);
    context.insert("next_page", &next);
    context.insert("no_navigation", &no_navigation);
    context.insert("is_static", &is_static);

    TEMPLATES
        .render("page.html", &context)
        .map_err(|e| format!("Template Error: {}", e))
}

pub async fn render_summary_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
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

pub async fn render_page_handler(
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
