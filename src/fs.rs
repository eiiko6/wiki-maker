use std::path::PathBuf;

use crate::{Page, WikiConfig};

pub async fn get_summary_data(docs_dir: &PathBuf, is_static: bool) -> Vec<Page> {
    let mut pages = Vec::new();
    if let Ok(mut entries) = tokio::fs::read_dir(docs_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("toml")
                || path.file_name().and_then(|s| s.to_str()) == Some("_changelog.toml")
            {
                continue;
            }

            let filename_str = if is_static {
                entry.file_name().to_string_lossy().into_owned()
            } else {
                path.file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| entry.file_name().to_string_lossy().into_owned())
            };

            let (title, category) = if let Ok(content) = tokio::fs::read_to_string(&path).await {
                if let Ok(config) = toml::from_str::<WikiConfig>(&content) {
                    (config.title, config.category)
                } else {
                    (filename_str.clone(), None)
                }
            } else {
                (filename_str.clone(), None)
            };

            pages.push(Page {
                filename: filename_str,
                title,
                category,
                datetime: "".to_string(),
            });
        }
    }

    pages.sort_by(|a, b| match (&a.category, &b.category) {
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (Some(c1), Some(c2)) => match c1.cmp(c2) {
            std::cmp::Ordering::Equal => a.title.cmp(&b.title),
            ord => ord,
        },
        (None, None) => a.title.cmp(&b.title),
    });
    pages
}

pub fn get_nav_links(dir: &PathBuf, current_file: &str) -> (Option<String>, Option<String>) {
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
