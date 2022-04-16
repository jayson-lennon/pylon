use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

use tracing::instrument;

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

#[cfg(test)]
mod test {
    use super::*;

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
