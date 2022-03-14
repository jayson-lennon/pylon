use scraper::{Html, Selector};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub fn find_assets<T: AsRef<str>>(html: T) -> Vec<String> {
    let selectors = [
        ("audio", "src"),
        ("embed", "src"),
        ("img", "src"),
        ("link", "href"),
        ("object", "data"),
        ("script", "src"),
        ("source", "src"),
        ("source", "srcset"),
        ("track", "src"),
        ("video", "src"),
    ];
    let doc = Html::parse_document(html.as_ref());

    let mut assets = vec![];
    for (tag, attr) in selectors {
        let selector = Selector::parse(tag).unwrap();
        for el in doc.select(&selector) {
            if let Some(url) = el.value().attr(attr) {
                assets.push(url.to_owned());
            }
        }
    }
    assets
}

pub fn get_all_html_paths<P: AsRef<Path>>(dir: P) -> io::Result<Vec<PathBuf>> {
    let dir = dir.as_ref();
    let mut html_paths = vec![];
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                html_paths.append(&mut get_all_html_paths(path)?);
            } else {
                if let Some(ext) = path.extension() {
                    if ext == "html" {
                        html_paths.push(path);
                    }
                }
            }
        }
    }
    Ok(html_paths)
}
