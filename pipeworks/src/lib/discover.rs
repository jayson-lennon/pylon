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

pub fn get_all_paths<P: AsRef<Path>>(
    dir: P,
    condition: &dyn Fn(&Path) -> bool,
) -> io::Result<Vec<PathBuf>> {
    let dir = dir.as_ref();
    let mut paths = vec![];
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                paths.append(&mut get_all_paths(path, condition)?);
            } else {
                if condition(path.as_ref()) {
                    paths.push(path);
                }
            }
        }
    }
    Ok(paths)
}

pub fn strip_root<P: AsRef<Path>>(path: P, root: P) -> PathBuf {
    let root_components = root.as_ref().iter().collect::<Vec<_>>();
    let path_components = path.as_ref().iter().collect::<Vec<_>>();
    let mut i = 0;
    while i < path_components.len() {
        if root_components.get(i) != path_components.get(i) {
            return PathBuf::from_iter(path_components.iter().skip(i));
        }
        i += 1;
    }
    PathBuf::from(path.as_ref())
}

pub fn template_paths_from_content_path<P, S>(
    content_path: P,
    content_root: P,
    template_name: S,
) -> Vec<PathBuf>
where
    P: AsRef<Path>,
    S: AsRef<str>,
{
    let content_path = strip_root(content_path, content_root);
    let mut content_path = content_path.iter().collect::<Vec<_>>();

    let mut paths = vec![];
    while content_path.len() > 0 {
        let mut path = PathBuf::from_iter(content_path.iter());
        path.push(template_name.as_ref());
        paths.push(path);
        content_path.pop();
    }
    paths
}

#[cfg(test)]
mod test {
    use super::template_paths_from_content_path;
    use std::path::PathBuf;

    #[test]
    fn gets_list_of_template_paths_for_given_content_path_when_ran_from_project_root() {
        let content_path = "blog/post1";
        let content_root = "src";
        let template_name = "single.tera";
        let template_paths =
            template_paths_from_content_path(content_path, content_root, template_name);
        assert_eq!(template_paths[0], PathBuf::from("blog/post1/single.tera"));
        assert_eq!(template_paths[1], PathBuf::from("blog/single.tera"));
    }

    #[test]
    fn gets_list_of_template_paths_for_given_content_path_when_paths_exist_elsewhere() {
        let content_path = "test/src/blog/post1";
        let content_root = "test/src";
        let template_name = "single.tera";
        let template_paths =
            template_paths_from_content_path(content_path, content_root, template_name);
        assert_eq!(template_paths[0], PathBuf::from("blog/post1/single.tera"));
        assert_eq!(template_paths[1], PathBuf::from("blog/single.tera"));
    }
}
