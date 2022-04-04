use std::path::Path;

use anyhow::anyhow;
use serde::Serialize;

#[derive(Clone, Debug, Default, Serialize, Eq, Hash, PartialEq)]
pub struct Uri(String);

impl Uri {
    pub fn new<S: Into<String>>(uri: S) -> Result<Self, anyhow::Error> {
        let uri = uri.into();
        if uri.starts_with('/') {
            Ok(Self(uri))
        } else {
            Err(anyhow!("Uri must start with a slash (/)"))
        }
    }

    pub fn from_path<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref();
        if path.has_root() {
            Self(path.display().to_string())
        } else {
            Self(format!("/{}", path.display()))
        }
    }

    pub fn as_str(&self) -> &str {
        self.as_ref()
    }
}

impl AsRef<str> for Uri {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl std::fmt::Display for Uri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod test {
    use super::Uri;

    #[test]
    fn require_slash() {
        let missing_slash = "my_uri/test";
        let uri = Uri::new(missing_slash);
        assert!(uri.is_err());
    }

    #[test]
    fn display_impl() {
        let uri = Uri::new("/some/page.txt").unwrap();
        assert_eq!(uri.to_string(), "/some/page.txt");
    }

    #[test]
    fn asref_impl() {
        let uri = Uri::new("/some/page.txt").unwrap();
        let as_str: &str = uri.as_ref();
        assert_eq!(as_str, "/some/page.txt");
    }

    #[test]
    fn as_str() {
        let uri = Uri::new("/some/page.txt").unwrap();
        assert_eq!(uri.as_str(), "/some/page.txt");
    }
}
