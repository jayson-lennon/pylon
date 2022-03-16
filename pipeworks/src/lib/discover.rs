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

pub fn get_template_paths_for_content_path<P: AsRef<Path>, S: AsRef<str>>(
    content_path: P,
    template_root: P,
    template_name: S,
) -> Vec<PathBuf> {
    let content_path = content_path.as_ref();
    let template_root = template_root.as_ref();
    let template_name = template_name.as_ref();

    let mut template_path = {
        let mut template_path = PathBuf::from(template_root);
        // build a path with the template root as the first directory
        for p in content_path.iter().skip(1) {
            template_path.push(p);
        }
        // add the target template name
        template_path.push(template_name);
        template_path
    };
    let mut paths = vec![];
    loop {
        paths.push(template_path.clone());

        template_path.pop();
        if !template_path.pop() {
            break;
        }

        if template_path.as_path() == template_root {
            break;
        }
        template_path.push(template_name);
    }
    paths
}

#[cfg(test)]
mod test {
    use super::get_template_paths_for_content_path;
    use std::path::PathBuf;

    #[test]
    fn gets_list_of_template_paths_for_given_content_path() {
        let content_path = PathBuf::from("src/blog/post1");
        let template_root = PathBuf::from("templates");
        let template_name = "single.tera";
        let template_paths =
            get_template_paths_for_content_path(content_path, template_root, template_name);
        assert_eq!(
            template_paths[0],
            PathBuf::from("templates/blog/post1/single.tera")
        );
        assert_eq!(
            template_paths[1],
            PathBuf::from("templates/blog/single.tera")
        );
    }
}
