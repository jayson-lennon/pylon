use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

pub fn get_all_paths(root: &Path, condition: &dyn Fn(&Path) -> bool) -> io::Result<Vec<PathBuf>> {
    let mut paths = vec![];
    if root.is_dir() {
        for entry in fs::read_dir(root)? {
            let path = entry?.path();
            if path.is_dir() {
                paths.append(&mut get_all_paths(&path, condition)?);
            } else {
                if condition(path.as_ref()) {
                    paths.push(path);
                }
            }
        }
    }
    Ok(paths)
}

pub fn find_assets<T: AsRef<str>>(html: T) -> HashSet<String> {
    use scraper::{Html, Selector};

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

    let mut assets = HashSet::new();
    for (tag, attr) in selectors {
        let selector = Selector::parse(tag).expect(&format!(
            "Error parsing CSS selector '{}'. This is a bug.",
            tag
        ));
        for el in doc.select(&selector) {
            if let Some(url) = el.value().attr(attr) {
                assets.insert(url.to_owned());
            }
        }
    }
    assets
}
