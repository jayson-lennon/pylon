// pub mod css_asset;
pub mod asset;
pub mod html_asset;
pub mod shortcode;

pub use asset::AssetPath;

use crate::Result;
use eyre::WrapErr;
use std::fs;

use std::path::Path;

use serde::Serialize;


use crate::AbsPath;

pub fn get_all_paths<P: AsRef<Path> + std::fmt::Debug>(
    root: P,
    condition: &dyn Fn(&Path) -> bool,
) -> Result<Vec<AbsPath>> {
    let root = root.as_ref();
    let mut paths = vec![];
    if root.is_dir() {
        for entry in fs::read_dir(root)
            .wrap_err_with(|| format!("Failed to read directory '{}'", root.display()))?
        {
            let path = entry
                .wrap_err_with(|| {
                    format!("Failed to read directory entry in '{}'", root.display())
                })?
                .path();
            if path.is_dir() {
                paths.append(&mut get_all_paths(&path, condition).wrap_err_with(|| {
                    format!("Failed to get all paths from '{}'", path.display())
                })?);
            } else if condition(path.as_ref()) {
                paths.push(AbsPath::new(&path).wrap_err_with(|| {
                    format!("Failed to convert '{}' to absolute path", path.display())
                })?);
            }
        }
    }
    Ok(paths)
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize)]
pub enum UrlType {
    Offsite,
    Absolute,
    Relative(String),
    InternalDoc(String),
}

pub fn get_url_type<S: AsRef<str>>(link: S) -> UrlType {
    use std::str::from_utf8;
    match link.as_ref().as_bytes() {
        // Internal doc: @/
        [b'@', b'/', target @ ..] => {
            // add the slashy back
            UrlType::InternalDoc(format!("/{}", from_utf8(target).unwrap()))
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

    #![allow(warnings, unused)]
    use super::*;

    #[test]
    fn get_link_target_identifies_internal_doc() {
        let internal_link = "@/some/path/page.md";
        let link = super::get_url_type(internal_link);
        match link {
            UrlType::InternalDoc(target) => assert_eq!(target, "/some/path/page.md"),
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
