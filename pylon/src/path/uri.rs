use derivative::Derivative;

use std::fmt;
use std::path::PathBuf;

use crate::core::engine::GlobalEnginePaths;
use crate::Result;
use crate::{pathmarker, CheckedFilePath, SysPath};
use anyhow::anyhow;
use serde::Serialize;

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

    pub fn to_checked_uri(&self, initiator: &CheckedFilePath<pathmarker::Html>) -> CheckedUri {
        CheckedUri::new(initiator, self.uri.clone())
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
    uri: String,
    html_src: CheckedFilePath<pathmarker::Html>,
}

impl CheckedUri {
    pub fn new<S: Into<String>>(initiator: &CheckedFilePath<pathmarker::Html>, uri: S) -> Self {
        let uri = uri.into();

        let mut abs_uri = PathBuf::new();

        if uri.starts_with('/') {
            abs_uri.push(&uri);
        } else {
            abs_uri.push("/");
            abs_uri.push(initiator.as_sys_path().pop().target());
            abs_uri.push(&uri);
        }

        Self {
            uri: abs_uri.to_string_lossy().to_string(),
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

    pub fn from_sys_path<S: Into<String>>(
        _engine_paths: GlobalEnginePaths,
        path: &SysPath,
        uri: S,
    ) -> Result<Self> {
        let checked_html = CheckedFilePath::new(path)?;
        Ok(Self::new(&checked_html, uri))
    }
}

impl From<CheckedFilePath<pathmarker::Html>> for CheckedUri {
    fn from(html_path: CheckedFilePath<pathmarker::Html>) -> Self {
        Self {
            uri: format!("/{}", html_path.as_sys_path().target().to_string_lossy()),
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
