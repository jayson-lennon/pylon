use derivative::Derivative;

use std::fmt;
use std::path::PathBuf;

use crate::Result;
use anyhow::anyhow;
use serde::Serialize;
use typed_path::{pathmarker, AbsPath, CheckedFilePath, RelPath, SysPath};

#[derive(Derivative, Serialize)]
#[derivative(Debug, Clone, Hash, PartialEq)]
pub struct Uri {
    uri: String,
}
impl Uri {
    pub fn new<S: Into<String>>(uri: S) -> Result<Self> {
        let uri = uri.into();

        let mut abs_uri = PathBuf::new();

        if uri.starts_with('/') {
            abs_uri.push(&uri);
            Ok(Self {
                uri: abs_uri.to_string_lossy().to_string(),
            })
        } else {
            Err(anyhow!("virtual URI must be absolute"))
        }
    }

    pub fn to_sys_path(&self, root: &AbsPath, base: &RelPath) -> Result<SysPath> {
        let uri_without_root_slash = &self.uri[1..];
        Ok(SysPath::new(
            root,
            base,
            &uri_without_root_slash.try_into()?,
        ))
    }

    pub fn to_checked_uri(&self, initiator: &CheckedFilePath<pathmarker::Html>) -> CheckedUri {
        CheckedUri::new(initiator, self)
    }

    pub fn as_str(&self) -> &str {
        self.uri.as_str()
    }

    pub fn into_boxed_str(&self) -> Box<str> {
        self.as_str().to_string().into_boxed_str()
    }
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.uri)
    }
}

#[derive(Derivative, Serialize)]
#[derivative(Debug, Clone, Hash, PartialEq)]
pub struct CheckedUri {
    uri: Uri,
    html_src: CheckedFilePath<pathmarker::Html>,
}

impl CheckedUri {
    pub fn new(initiator: &CheckedFilePath<pathmarker::Html>, uri: &Uri) -> Self {
        Self {
            uri: uri.clone(),
            html_src: initiator.clone(),
        }
    }

    pub fn as_str(&self) -> &str {
        self.uri.as_str()
    }

    pub fn into_boxed_str(&self) -> Box<str> {
        self.as_str().to_string().into_boxed_str()
    }

    pub fn html_src(&self) -> &CheckedFilePath<pathmarker::Html> {
        &self.html_src
    }

    pub fn to_sys_path(&self, root: &AbsPath, base: &RelPath) -> Result<SysPath> {
        self.uri.to_sys_path(root, base)
    }
}

impl From<CheckedFilePath<pathmarker::Html>> for CheckedUri {
    fn from(html_path: CheckedFilePath<pathmarker::Html>) -> Self {
        // slash is prepended to the URI. creationwill always succeed
        let uri = Uri::new(format!(
            "/{}",
            html_path.as_sys_path().target().to_string_lossy()
        ))
        .unwrap();
        Self {
            uri,
            html_src: html_path,
        }
    }
}

impl Eq for CheckedUri {}

impl fmt::Display for CheckedUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// #[cfg(test)]
// mod test {
//     #![allow(unused_variables)]

//     use temptree::temptree;

//     use super::*;
//     use crate::test::rel;

//     #[test]
//     fn rel_file_works() {
//         let tree = temptree! {
//           "rules.rhai": "",
//           templates: {},
//           target: {
//               folder: {},
//               "page.html": "",
//           },
//           src: {
//               "page.md": "",
//           },
//           syntax_themes: {}
//         };
//         let paths = crate::test::default_test_paths(&tree);
//         let path = SysPath::new(paths, rel!("target"), rel!("page.html")).unwrap();
//         let relfile: RelFile = path
//             .try_into()
//             .expect("failed to create relative file from syspath");
//     }
// }
