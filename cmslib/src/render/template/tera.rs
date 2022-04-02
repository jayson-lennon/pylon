use std::path::{Path, PathBuf};
use tera::Tera;

use super::TemplateName;

#[derive(Debug)]
pub struct TeraRenderer {
    renderer: Tera,
}

impl TeraRenderer {
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        let mut root = PathBuf::from(root.as_ref());
        root.push("**/*.tera");

        let r = Tera::new(root.to_str().expect("non UTF-8 characters in path"))
            .expect("error initializing template rendering engine");

        Self { renderer: r }
    }
    pub fn render(
        &self,
        template: &TemplateName,
        context: &tera::Context,
    ) -> Result<String, tera::Error> {
        Ok(self.renderer.render(template.as_ref(), context)?)
    }

    pub fn get_template_names(&self) -> impl Iterator<Item = &str> {
        self.renderer.get_template_names()
    }

    pub fn reload(&mut self) -> Result<(), anyhow::Error> {
        Ok(self.renderer.full_reload()?)
    }
}
