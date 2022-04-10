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
        self.renderer.render(template.as_ref(), context)
    }

    pub fn get_template_names(&self) -> impl Iterator<Item = &str> {
        self.renderer.get_template_names()
    }

    pub fn reload(&mut self) -> Result<(), anyhow::Error> {
        Ok(self.renderer.full_reload()?)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use temptree::temptree;

    #[test]
    fn renders_with_valid_template() {
        let tree = temptree! {
            templates: {
                "basic.tera": "data: {{content}}"
            }
        };

        let template_root = tree.path().join("templates");

        let renderer = TeraRenderer::new(template_root);

        let mut ctx = tera::Context::new();
        ctx.insert("content", "testing");

        let rendered = renderer.render(&"basic.tera".into(), &ctx).unwrap();

        assert_eq!(rendered.as_str(), "data: testing");
    }

    #[test]
    fn render_fails_when_missing_content_data() {
        let tree = temptree! {
            templates: {
                "basic.tera": "data: {{content}}"
            }
        };

        let template_root = tree.path().join("templates");

        let renderer = TeraRenderer::new(template_root);

        let ctx = tera::Context::new();

        let rendered = renderer.render(&"basic.tera".into(), &ctx);

        assert!(rendered.is_err());
    }
}
