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

pub fn get_template_paths_for_content_path<P, S>(
    content_path: P,
    template_root: P,
    template_name: S,
) -> Vec<PathBuf>
where
    P: AsRef<Path>,
    S: AsRef<str>,
{
    let content_path = content_path.as_ref();
    let template_root = template_root.as_ref();
    let template_name = template_name.as_ref();

    let content_components = content_path.iter().collect::<Vec<_>>();
    let template_root_components = template_root.iter().collect::<Vec<_>>();

    // This is 0 when the content path and template root are both present in the CWD.
    // Otherwise, it indicates how many directories deep both paths are. This happens
    // when the generator is ran outside the projects directory, or if the
    // directories are nested inside the project directory. See test code for examples.
    let content_dir_start = {
        let mut i = 0;
        loop {
            if content_components.get(i) != template_root_components.get(i) {
                break;
            }
            i += 1;
        }
        i
    };

    let mut template_path = content_components;
    template_path[content_dir_start] = template_root_components[template_root_components.len() - 1];
    template_path.push(template_name.as_ref());

    let mut paths = vec![];
    while template_path.len() > template_root_components.len() {
        paths.push(PathBuf::from_iter(template_path.iter()));
        template_path.remove(template_path.len() - 2); // -2 so we don't remove the filename
    }
    paths
}

#[cfg(test)]
mod test {
    use super::get_template_paths_for_content_path;
    use std::path::PathBuf;

    #[test]
    fn gets_list_of_template_paths_for_given_content_path_when_ran_from_project_root() {
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

    #[test]
    fn gets_list_of_template_paths_for_given_content_path_when_paths_exist_elsewhere() {
        let content_path = PathBuf::from("test/src/blog/post1");
        let template_root = PathBuf::from("test/templates");
        let template_name = "single.tera";
        let template_paths =
            get_template_paths_for_content_path(content_path, template_root, template_name);
        assert_eq!(
            template_paths[0],
            PathBuf::from("test/templates/blog/post1/single.tera")
        );
        assert_eq!(
            template_paths[1],
            PathBuf::from("test/templates/blog/single.tera")
        );
    }
}
