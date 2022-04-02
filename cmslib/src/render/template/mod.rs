mod tera;
use serde::{Deserialize, Serialize};

pub use crate::render::template::tera::TeraRenderer;

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
