use serde::{Deserialize, Serialize};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, Serialize, PartialOrd, PartialEq)]
pub struct CmsPath {
    root: PathBuf,
    path: PathBuf,
}

impl CmsPath {
    pub fn new<P: AsRef<Path>>(root: P, path: P) -> Self {
        Self {
            root: PathBuf::from(root.as_ref()),
            path: PathBuf::from(path.as_ref()),
        }
    }
    pub fn file_name(&self) -> Option<&OsStr> {
        self.path.file_name()
    }

    pub fn root(&self) -> &Path {
        self.root.as_path()
    }

    pub fn to_full_path(&self) -> PathBuf {
        let mut full_path = PathBuf::from(&self.root);
        full_path.push(&self.path);
        full_path
    }

    pub fn with_filename<N: AsRef<Path>>(&self, file_name: N) -> Self {
        let mut new_path = PathBuf::from(&self.path);
        new_path.set_file_name(file_name.as_ref());
        Self {
            root: PathBuf::from(&self.root),
            path: new_path,
        }
    }

    pub fn with_root<P: AsRef<Path>>(&self, root: P) -> Self {
        Self {
            root: PathBuf::from(root.as_ref()),
            path: PathBuf::from(&self.path),
        }
    }

    pub fn to_template_path<P: AsRef<Path>>(&self, template_root: P) -> Self {
        self.with_root(template_root)
    }

    pub fn to_output_path<P: AsRef<Path>>(&self, output_root: P) -> Self {
        self.with_root(output_root)
    }

    pub fn to_source_path<P: AsRef<Path>>(&self, source_root: P) -> Self {
        self.with_root(source_root)
    }

    pub fn pop_parent(&mut self) -> bool {
        let last = self.path.iter().last().map(|s| s.to_owned());
        if let Some(last) = last {
            self.path.pop(); // remove last component
            let parent_popped = self.path.pop();
            self.path.push(last);
            if !parent_popped {
                false
            } else {
                true
            }
        } else {
            false
        }
    }
}

impl From<CmsPath> for PathBuf {
    fn from(cmspath: CmsPath) -> Self {
        cmspath.to_full_path()
    }
}

impl From<&CmsPath> for PathBuf {
    fn from(cmspath: &CmsPath) -> Self {
        cmspath.to_full_path()
    }
}

pub fn strip_root<P: AsRef<Path>>(root: P, path: P) -> PathBuf {
    let root = root.as_ref().iter().collect::<Vec<_>>();
    let path = path.as_ref().iter().collect::<Vec<_>>();

    let mut i = 0;
    while root.get(i) == path.get(i) {
        i += 1;
    }
    PathBuf::from_iter(path[i..].iter())
}

#[cfg(test)]
mod test {
    use super::{strip_root, CmsPath};
    use std::path::PathBuf;

    #[test]
    fn pops_parent() {
        let mut path = CmsPath::new("root", "a/b/c.txt");

        assert_eq!(path.pop_parent(), true);
        assert_eq!(path.path, PathBuf::from("a/c.txt"));

        assert_eq!(path.pop_parent(), true);
        assert_eq!(path.path, PathBuf::from("c.txt"));

        assert_eq!(path.pop_parent(), false);
        assert_eq!(path.path, PathBuf::from("c.txt"));

        assert_eq!(path.to_full_path(), PathBuf::from("root/c.txt"))
    }

    #[test]
    fn strips_root() {
        let stripped = strip_root("a/b", "a/b/c/d");
        assert_eq!(stripped, PathBuf::from("c/d"));

        let stripped = strip_root("", "a/b/c/d");
        assert_eq!(stripped, PathBuf::from("a/b/c/d"));

        let stripped = strip_root("a", "");
        assert_eq!(stripped, PathBuf::from(""));
    }
}
