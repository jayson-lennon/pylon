use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

use tracing::instrument;
use tracing::trace;

use crate::page::{LinkedAsset, LinkedAssets};
use crate::render::page::RenderedPage;

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

#[instrument(level = "trace", ret)]
fn assets_in_html<T: AsRef<str> + std::fmt::Debug>(html: T) -> HashSet<String> {
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

#[instrument(skip_all)]
/// This function rewrites the asset location if applicable
pub fn linked_assets(pages: &mut [RenderedPage]) -> Result<LinkedAssets, anyhow::Error> {
    trace!("searching for linked external assets in rendered pages");
    let mut all_assets = HashSet::new();
    for page in pages.iter() {
        let page_assets = assets_in_html(&page.html)
            .iter()
            .map(|asset| {
                if asset.starts_with("/") {
                    // absolute path assets don't need any modifications
                    LinkedAsset::new(PathBuf::from(asset))
                } else {
                    // relative path assets need the parent directory of the page applied
                    let mut target = page
                        .target
                        .as_target()
                        .parent()
                        .expect("should have a parent")
                        .to_path_buf();
                    target.push(asset);
                    LinkedAsset::new(target)
                }
            })
            .collect::<HashSet<_>>();
        all_assets.extend(page_assets);
    }

    Ok(LinkedAssets::new(all_assets))
}
