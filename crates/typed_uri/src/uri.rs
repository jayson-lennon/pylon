use derivative::Derivative;

use std::fmt;
use std::path::PathBuf;

use crate::Result;
use eyre::{eyre, WrapErr};
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
            Err(eyre!("virtual URI must be absolute"))
        }
    }

    pub fn to_sys_path(&self, root: &AbsPath, base: &RelPath) -> Result<SysPath> {
        let uri_without_root_slash = &self.uri[1..];
        Ok(SysPath::new(
            root,
            base,
            &uri_without_root_slash.try_into().wrap_err_with(|| {
                format!(
                    "Failed converting Uri '{}' to SysPath with root '{}' and base '{}'",
                    self.uri, root, base
                )
            })?,
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

    pub fn to_target_sys_path(&self, root: &AbsPath, base: &RelPath) -> Result<SysPath> {
        self.uri.to_sys_path(root, base)
    }

    pub fn src_sys_path(&self) -> &SysPath {
        self.html_src.as_sys_path()
    }

    pub fn as_unchecked(&self) -> &Uri {
        &self.uri
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

#[cfg(test)]
mod test {
    #![allow(warnings, unused)]
    use temptree::temptree;

    use super::*;
    use crate::test::{abs, rel};

    #[test]
    fn checked_uri_as_str() {
        let tree = temptree! {
          "test.html": "",
        };
        let path = SysPath::new(abs!(tree.path()), rel!(""), rel!("test.html"));
        let uri = Uri::new("/page.html").unwrap();
        let checked_path = path.try_into().expect("failed to make checked file path");
        let checked_uri = CheckedUri::new(&checked_path, &uri);
        assert_eq!(checked_uri.as_str(), "/page.html");
    }

    #[test]
    fn checked_uri_into_boxed_str() {
        let tree = temptree! {
          "test.html": "",
        };
        let path = SysPath::new(abs!(tree.path()), rel!(""), rel!("test.html"));
        let uri = Uri::new("/page.html").unwrap();
        let checked_path = path.try_into().expect("failed to make checked file path");
        let checked_uri = CheckedUri::new(&checked_path, &uri);
        assert_eq!(checked_uri.into_boxed_str(), "/page.html".into());
    }

    #[test]
    fn checked_uri_display() {
        let tree = temptree! {
          "test.html": "",
        };
        let path = SysPath::new(abs!(tree.path()), rel!(""), rel!("test.html"));
        let uri = Uri::new("/page.html").unwrap();
        let checked_path = path.try_into().expect("failed to make checked file path");
        let checked_uri = CheckedUri::new(&checked_path, &uri);
        assert_eq!(checked_uri.to_string(), "/page.html".to_owned());
    }

    #[test]
    fn checked_uri_as_unchecked() {
        let tree = temptree! {
          "test.html": "",
        };
        let path = SysPath::new(abs!(tree.path()), rel!(""), rel!("test.html"));
        let uri = Uri::new("/page.html").unwrap();
        let checked_path = path.try_into().expect("failed to make checked file path");
        let checked_uri = CheckedUri::new(&checked_path, &uri);
        assert_eq!(checked_uri.as_unchecked(), &uri);
    }

    #[test]
    fn checked_uri_src_sys_path() {
        let tree = temptree! {
          "test.html": "",
        };
        let path = SysPath::new(abs!(tree.path()), rel!(""), rel!("test.html"));
        let uri = Uri::new("/page.html").unwrap();
        let checked_path = path
            .clone()
            .try_into()
            .expect("failed to make checked file path");
        let checked_uri = CheckedUri::new(&checked_path, &uri);
        assert_eq!(checked_uri.src_sys_path(), &path);
    }

    #[test]
    fn checked_uri_to_target_sys_path() {
        let tree = temptree! {
          "test.html": "",
        };
        let path = SysPath::new(abs!(tree.path()), rel!(""), rel!("test.html"));
        let uri = Uri::new("/page.html").unwrap();
        let checked_path = path.try_into().expect("failed to make checked file path");
        let checked_uri = CheckedUri::new(&checked_path, &uri);
        let sys_path = checked_uri
            .to_target_sys_path(abs!(tree.path()), rel!(""))
            .expect("failed to conver URI target to SysPath");
        assert_eq!(
            sys_path.to_absolute_path(),
            abs!(tree.path()).join(rel!("page.html"))
        )
    }

    #[test]
    fn new_uri() {
        let uri = "/test";
        Uri::new(uri).expect("failed to make new URI");
    }

    #[test]
    fn uri_as_str() {
        let uri = "/test";
        let uri = Uri::new(uri).expect("failed to make new URI");
        assert_eq!(uri.as_str(), "/test");
    }

    #[test]
    fn uri_into_boxed_str() {
        let uri = "/test";
        let uri = Uri::new(uri).expect("failed to make new URI");
        assert_eq!(uri.into_boxed_str(), "/test".into());
    }

    #[test]
    fn new_uri_fails_if_not_absolute() {
        let uri = "test";
        let uri = Uri::new(uri);
        assert!(uri.is_err());
    }

    #[test]
    fn uri_to_sys_path() {
        let uri = "/test";
        let uri = Uri::new(uri).unwrap();
        let sys_path = uri
            .to_sys_path(abs!("/"), rel!(""))
            .expect("failed to create SysPath from Uri");
    }

    #[test]
    fn uri_to_sys_path_fails_with_broken_path() {
        let uri = "//test";
        let uri = Uri::new(uri).unwrap();
        let sys_path = uri.to_sys_path(abs!("/"), rel!(""));
        assert!(sys_path.is_err());
    }
}
