#![allow(dead_code)]

pub mod core;
pub mod devserver;
pub mod discover;
pub mod frontmatter;
pub mod gctx;
pub mod page;
pub mod pagestore;
pub mod pipeline;
pub mod render;
pub mod site_context;
pub mod util;

pub use render::Renderers;

use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Serialize, PartialEq, Hash, Eq, Clone, Default)]
pub struct CanonicalPath(String);

impl CanonicalPath {
    pub fn new<P: AsRef<str>>(path: P) -> Self {
        let path = path.as_ref();
        if !path.starts_with("/") {
            Self(format!("/{}", path))
        } else {
            Self(path.to_owned())
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn relative(&self) -> &str {
        &self.0[1..]
    }

    pub fn without_file_name(&self) -> &str {
        if let Some(index) = self.0.rfind("/") {
            &self.0[..index]
        } else {
            &self.0
        }
    }

    pub fn relative_without_file_name(&self) -> &str {
        if let Some(index) = self.0.rfind("/") {
            &self.0[1..index]
        } else {
            &self.0
        }
    }

    pub fn parent(&self) -> Self {
        let path = PathBuf::from(&self.0);
        Self::new(
            &path
                .as_path()
                .parent()
                .map(|p| p.to_string_lossy())
                .unwrap_or_else(|| std::borrow::Cow::Borrowed("")),
        )
    }

    pub fn to_string(&self) -> String {
        self.0.clone()
    }
}
