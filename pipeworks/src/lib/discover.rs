use crate::cmspath::{self, CmsPath};
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

pub fn get_all_paths(
    cmspath: CmsPath,
    condition: &dyn Fn(&Path) -> bool,
) -> io::Result<Vec<CmsPath>> {
    let mut paths = vec![];
    let fullpath = cmspath.to_full_path();
    if fullpath.is_dir() {
        for entry in fs::read_dir(fullpath)? {
            let path = entry?.path();
            if path.is_dir() {
                let path = {
                    let stripped = cmspath::strip_root(cmspath.root(), &path);
                    CmsPath::new(cmspath.root(), &stripped)
                };
                paths.append(&mut get_all_paths(path, condition)?);
            } else {
                if condition(path.as_ref()) {
                    let path = {
                        let stripped = cmspath::strip_root(cmspath.root(), &path);
                        CmsPath::new(cmspath.root(), &stripped)
                    };
                    paths.push(path);
                }
            }
        }
    }
    Ok(paths)
}

pub fn possible_template_paths<P, S>(
    path: &CmsPath,
    template_root: P,
    template_name: S,
) -> Vec<CmsPath>
where
    P: AsRef<Path>,
    S: AsRef<str>,
{
    let mut paths = vec![];
    let mut path = path
        .with_root(template_root)
        .with_filename(template_name.as_ref());
    paths.push(path.clone());
    while path.pop_parent() {
        paths.push(path.clone());
    }
    paths
}

#[cfg(test)]
mod test {
    use super::possible_template_paths;
    use crate::cmspath::CmsPath;

    #[test]
    fn gets_possible_template_paths() {
        let path = CmsPath::new("src", "blog/post1/mypost.md");
        let template_root = "templates";
        let template_name = "single.tera";
        let template_paths = possible_template_paths(&path, template_root, template_name);
        assert_eq!(
            template_paths[0],
            CmsPath::new("templates", "blog/post1/single.tera")
        );
        assert_eq!(
            template_paths[1],
            CmsPath::new("templates", "blog/single.tera")
        );
        assert_eq!(template_paths[2], CmsPath::new("templates", "single.tera"));
    }
}
