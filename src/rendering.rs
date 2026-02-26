use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse},
};
use pulldown_cmark::{
    CodeBlockKind, CowStr, Event, LinkType, Options, Parser as MarkdownParser, Tag, TagEnd, html,
};
use pulldown_cmark_escape::escape_html;
use std::path::PathBuf;
use std::sync::Arc;
use syntect::html::highlighted_html_for_string;
use tera::Context;

use crate::{
    AppState, SYNTAX_SET, TEMPLATES, THEME_SET, WikiConfig, get_nav_links, get_summary_data,
};

pub struct Renderer<'a> {
    inner: MarkdownParser<'a>,
    is_static: bool,
}

impl<'a> Renderer<'a> {
    pub fn new(inner: MarkdownParser<'a>, is_static: bool) -> Self {
        Self { inner, is_static }
    }
}

impl<'a> Iterator for Renderer<'a> {
    type Item = Event<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let event = self.inner.next()?;

        match event {
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) if !matches!(link_type, LinkType::Email) && self.is_static => {
                let mut new_url = dest_url.to_string();

                if !new_url.contains("://")
                    && !new_url.starts_with("mailto:")
                    && !new_url.starts_with('#')
                    && !new_url.ends_with(".html")
                {
                    new_url.push_str(".html");
                }

                Some(Event::Start(Tag::Link {
                    link_type,
                    dest_url: CowStr::Boxed(new_url.into_boxed_str()),
                    title,
                    id,
                }))
            }

            Event::Start(Tag::CodeBlock(kind)) => {
                let mut code_content = String::new();

                while let Some(inner_event) = self.inner.next() {
                    match inner_event {
                        Event::End(TagEnd::CodeBlock) => break,
                        Event::Text(code) => code_content.push_str(&code),
                        _ => {}
                    }
                }

                let lang = match kind {
                    CodeBlockKind::Indented => "text",
                    CodeBlockKind::Fenced(ref language) => language.as_ref(),
                };

                let rendered_html = render_code_to_html(&code_content, lang);

                let mut escaped_code = String::new();
                let _ = escape_html(&mut escaped_code, &code_content);

                let rendered_html =
                    rendered_html.replace("<pre", &format!("<pre data-code=\"{}\"", escaped_code));

                Some(Event::Html(CowStr::Boxed(rendered_html.into_boxed_str())))
            }
            _ => return Some(event),
        }
    }
}

pub fn render_code_to_html(code: &str, lang: &str) -> String {
    let syntax = SYNTAX_SET
        .find_syntax_by_token(lang)
        .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());

    let theme = &THEME_SET.themes["Catppuccin Macchiato"];

    highlighted_html_for_string(code, &SYNTAX_SET, syntax, theme)
        .unwrap_or_else(|_| format!("<pre><code>{}</code></pre>", code))
}

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
    let toml_path = docs_dir
        .join(filename)
        .canonicalize()
        .map_err(|_| "Not found")?;

    let canonical_root = docs_dir
        .canonicalize()
        .map_err(|_| "Server Error: Invalid Root")?;

    if !toml_path.starts_with(&canonical_root) {
        return Err("Access Denied".to_string());
    }

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
    let renderer = Renderer::new(parser, is_static);
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

    let main_image_path = config.image.as_ref().map(|img| format!("images/{}", img));

    let mut context = Context::new();
    context.insert("title", &config.title);
    context.insert("content", &html_output);
    context.insert("infobox", &infobox_list);
    context.insert("main_image", &main_image_path);
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
    let pages = get_summary_data(&state.docs_dir, false).await;
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
