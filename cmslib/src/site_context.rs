use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SiteContext {
    pub title: String,
}

impl SiteContext {
    pub fn new<S: Into<String>>(title: S) -> Self {
        Self {
            title: title.into(),
        }
    }
}
