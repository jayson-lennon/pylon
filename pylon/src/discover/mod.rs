pub mod css_asset;
pub mod html_asset;

use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

use tracing::instrument;

use crate::core::Uri;

#[instrument(skip(condition), ret)]
pub fn get_all_paths(root: &Path, condition: &dyn Fn(&Path) -> bool) -> io::Result<Vec<PathBuf>> {
    let mut paths = vec![];
    if root.is_dir() {
        for entry in fs::read_dir(root)? {
            let path = entry?.path();
            if path.is_dir() {
                paths.append(&mut get_all_paths(&path, condition)?);
            } else if condition(path.as_ref()) {
                paths.push(path);
            }
        }
    }
    Ok(paths)
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum UrlType {
    Offsite,
    Absolute,
    Relative(String),
    InternalDoc(Uri),
}

pub fn get_url_type<S: AsRef<str>>(link: S) -> UrlType {
    use std::str::from_utf8;
    match link.as_ref().as_bytes() {
        // Internal doc: @/
        [b'@', b'/', target @ ..] => {
            UrlType::InternalDoc(Uri::from_path(from_utf8(target).unwrap()))
        }
        // Absolute: /
        [b'/', ..] => UrlType::Absolute,
        // Relative: ./
        [b'h', b't', b't', b'p', b':', b'/', b'/', ..] => UrlType::Offsite,
        [b'h', b't', b't', b'p', b's', b':', b'/', b'/', ..] => UrlType::Offsite,
        [target @ ..] => UrlType::Relative(from_utf8(target).unwrap().to_owned()),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn get_link_target_identifies_internal_doc() {
        let internal_link = "@/some/path/page.md";
        let link = super::get_url_type(internal_link);
        match link {
            UrlType::InternalDoc(uri) => assert_eq!(uri, Uri::from_path("some/path/page.md")),
            #[allow(unreachable_patterns)]
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn get_link_target_identifies_absolute_target() {
        let abs_target = "/some/path/page.md";
        let link = super::get_url_type(abs_target);
        match link {
            UrlType::Absolute => (),
            #[allow(unreachable_patterns)]
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn get_link_target_identifies_relative_target() {
        let rel_target = "some/path/page.md";
        let link = super::get_url_type(rel_target);
        match link {
            UrlType::Relative(target) => assert_eq!(target, "some/path/page.md"),
            #[allow(unreachable_patterns)]
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn get_link_target_identifies_offsite_target() {
        let offsite_target = "http://example.com";
        let link = super::get_url_type(offsite_target);
        match link {
            UrlType::Offsite => (),
            #[allow(unreachable_patterns)]
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn gets_all_paths_with_subdirs() {
        use temptree::temptree;

        let tree = temptree! {
          file: "",
          a: {
              b: {
                  c: {
                      file: ""
                  }
              },
              b2: {
                  file: ""
              }
          }
        };
        let root = tree.path().join("a");
        let paths = get_all_paths(&root, &|_| true).unwrap();

        assert_eq!(paths.len(), 2);
    }
}
