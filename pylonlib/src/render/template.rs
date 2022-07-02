mod tera;

pub use crate::render::template::tera::TeraRenderer;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct TemplateName(String);

impl TemplateName {
    pub fn new<S: Into<String>>(name: S) -> Self {
        Self(name.into())
    }
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for TemplateName {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl From<String> for TemplateName {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for TemplateName {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl std::fmt::Display for TemplateName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
#[cfg(test)]
mod test {

    #![allow(warnings, unused)]
    use super::*;

    #[test]
    fn template_name_as_str() {
        let name = "test";
        let template = TemplateName::new(name);
        assert_eq!(template.as_str(), name);
    }

    #[test]
    fn template_name_into_string() {
        let name = "test";
        let template = TemplateName::new(name);
        assert_eq!(template.into_string(), String::from(name));
    }

    #[test]
    fn template_name_as_ref() {
        let name = "test";
        let template = TemplateName::new(name);
        assert_eq!(template.as_ref(), name);
    }

    #[test]
    fn template_name_from_str() {
        let name = "test";
        let template = TemplateName::from(name);
        assert_eq!(template.as_ref(), name);
    }

    #[test]
    fn template_name_from_string() {
        let name = String::from("test");
        let template = TemplateName::from(name);
        assert_eq!(template.as_ref(), "test");
    }
}
