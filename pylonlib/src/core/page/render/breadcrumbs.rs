// start with current page as the "last" crumb
// check the parent directory for an "index.md"
//   if found -> this is the next breadcrumb
//   not found -> go up one directory; loop

use std::{ffi::OsStr, path::PathBuf};

use crate::core::{library::SearchKey, Library, Page};

pub fn generate<'p>(library: &'p Library, page: &Page) -> Vec<&'p Page> {
    let mut crumbs = vec![library.get_with_key(page.page_key).unwrap()];

    let mut path = {
        // If the doc is an `index` doc, then we want to jump up one directory,
        // so we `pop` twice to remove the filename (index.md) and the parent
        // directory. This results in the next ancestor being the directory
        // processed.
        if page.path().as_sys_path().file_name() == OsStr::new("index.md") {
            page.path().as_sys_path().pop().pop()
        } else {
            // Remove the current file name, so we can check for `index.md`
            // in the current directory.
            page.path().as_sys_path().pop()
        }
    };

    while path.target().file_name().is_some() {
        let search_key = {
            let key = PathBuf::from("/")
                .join(path.target())
                .join("index.md")
                .to_string_lossy()
                .to_string();
            SearchKey::new(key)
        };
        if let Some(page) = library.get(&search_key) {
            crumbs.push(page);
        }
        path = path.pop();
    }

    if let Some(page) = library.get(&SearchKey::new("/index.md")) {
        crumbs.push(page);
    }

    crumbs.into_iter().rev().collect()
}

#[cfg(test)]
mod test {
    #![allow(warnings, unused)]

    use tempfile::TempDir;
    use temptree::temptree;

    use crate::core::{
        library::SearchKey,
        page::test_page::{doc::MINIMAL, new_page, new_page_with_tree},
        Library, PageKey,
    };

    fn setup() -> (TempDir, Library) {
        let tree = temptree! {
            "rules.rhai": "",
            templates: {
                "default.tera": "",
            },
            target: {},
            src: {
                "page0.md": "",
                "index.md": "",
                "1": {
                    "index.md": "",
                    "2": {
                        "page2.md": "",
                        "3": {
                            "index.md": "",
                            "page3.md": "",
                        },
                    },
                },
            },
            syntax_themes: {},
        };

        let page0 = new_page_with_tree(&tree, &tree.path().join("src/page0.md"), MINIMAL).unwrap();
        let page1 =
            new_page_with_tree(&tree, &tree.path().join("src/1/page1.md"), MINIMAL).unwrap();
        let page2 =
            new_page_with_tree(&tree, &tree.path().join("src/1/2/page2.md"), MINIMAL).unwrap();
        let page3 =
            new_page_with_tree(&tree, &tree.path().join("src/1/2/3/page3.md"), MINIMAL).unwrap();

        let index0 = new_page_with_tree(&tree, &tree.path().join("src/index.md"), MINIMAL).unwrap();
        let index1 =
            new_page_with_tree(&tree, &tree.path().join("src/1/index.md"), MINIMAL).unwrap();
        let index2 =
            new_page_with_tree(&tree, &tree.path().join("src/1/2/3/index.md"), MINIMAL).unwrap();

        let mut library = Library::new();
        library.insert(page0);
        library.insert(page1);
        library.insert(page2);
        library.insert(page3);

        library.insert(index0);
        library.insert(index1);
        library.insert(index2);

        (tree, library)
    }

    #[test]
    fn top_level_with_index() {
        let (tree, library) = setup();

        let index0 = library.get(&SearchKey::new("/index.md")).unwrap();
        let page0 = library.get(&SearchKey::new("/page0.md")).unwrap();

        let crumbs = super::generate(&library, &page0);
        assert_eq!(crumbs.len(), 2);
        assert_eq!(crumbs[0].path(), index0.path());
        assert_eq!(crumbs[1].path(), page0.path());
    }

    #[test]
    fn one_level_deep_is_index() {
        let (tree, library) = setup();

        let index0 = library.get(&SearchKey::new("/index.md")).unwrap();
        let index1 = library.get(&SearchKey::new("/1/index.md")).unwrap();

        let crumbs = super::generate(&library, &index1);
        assert_eq!(crumbs.len(), 2);

        assert_eq!(crumbs[0].path(), index0.path());
        assert_eq!(crumbs[1].path(), index1.path());
    }

    #[test]
    fn two_levels_deep_without_index() {
        let (tree, library) = setup();

        let index0 = library.get(&SearchKey::new("/index.md")).unwrap();
        let index1 = library.get(&SearchKey::new("/1/index.md")).unwrap();
        let page2 = library.get(&SearchKey::new("/1/2/page2.md")).unwrap();

        let crumbs = super::generate(&library, &page2);
        assert_eq!(crumbs.len(), 3);

        assert_eq!(crumbs[0].path(), index0.path());
        assert_eq!(crumbs[1].path(), index1.path());
        assert_eq!(crumbs[2].path(), page2.path());
    }

    #[test]
    fn three_levels_deep_skips_missing_index() {
        let (tree, library) = setup();

        // we should jump from /1/2/3 to /1 since there is no /1/2/index.md
        let index0 = library.get(&SearchKey::new("/index.md")).unwrap();
        let index1 = library.get(&SearchKey::new("/1/index.md")).unwrap();
        let index2 = library.get(&SearchKey::new("/1/2/3/index.md")).unwrap();
        let page3 = library.get(&SearchKey::new("/1/2/3/page3.md")).unwrap();

        let crumbs = super::generate(&library, &page3);
        assert_eq!(crumbs.len(), 4);

        assert_eq!(crumbs[0].path(), index0.path());
        assert_eq!(crumbs[1].path(), index1.path());
        assert_eq!(crumbs[2].path(), index2.path());
        assert_eq!(crumbs[3].path(), page3.path());
    }
}
