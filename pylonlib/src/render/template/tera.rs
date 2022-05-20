use eyre::{eyre, WrapErr};
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tera::Tera;

use crate::Result;

use super::TemplateName;

#[derive(Debug)]
pub struct TeraRenderer {
    renderer: Arc<Mutex<Tera>>,
}

impl TeraRenderer {
    pub fn new<P: AsRef<Path>>(root: P) -> Result<Self> {
        let mut root = PathBuf::from(root.as_ref());
        root.push("**/*.tera");

        let r = Tera::new(
            root.to_str()
                .ok_or_else(|| eyre!("non UTF-8 characters in path"))?,
        )
        .with_context(|| "error initializing template rendering engine")?;

        Ok(Self {
            renderer: Arc::new(Mutex::new(r)),
        })
    }
    pub fn render(&self, template: &TemplateName, context: &tera::Context) -> Result<String> {
        let renderer = self.renderer.lock();
        Ok(renderer.render(template.as_ref(), context)?)
    }

    pub fn one_off<S: AsRef<str>>(&self, input: S, context: &tera::Context) -> Result<String> {
        let mut renderer = self.renderer.lock();
        Ok(renderer.render_str(input.as_ref(), context)?)
    }

    pub fn get_template_names(&self) -> Vec<String> {
        let renderer = self.renderer.lock();
        renderer
            .get_template_names()
            .map(|s| s.to_string())
            .collect()
    }

    pub fn reload(&mut self) -> Result<()> {
        let mut renderer = self.renderer.lock();
        Ok(renderer.full_reload()?)
    }
}

#[cfg(test)]
mod test {

    #![allow(warnings, unused)]
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

        let template_renderer =
            TeraRenderer::new(template_root).expect("failed to create renderer");

        let mut ctx = tera::Context::new();
        ctx.insert("content", "testing");

        let rendered = template_renderer
            .render(&"basic.tera".into(), &ctx)
            .unwrap();

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

        let template_renderer =
            TeraRenderer::new(template_root).expect("failed to create renderer");

        let ctx = tera::Context::new();

        let rendered = template_renderer.render(&"basic.tera".into(), &ctx);

        assert!(rendered.is_err());
    }

    #[test]
    fn renders_one_off() {
        let tree = temptree! {
            templates: { }
        };

        let template_root = tree.path().join("templates");

        let template_renderer =
            TeraRenderer::new(template_root).expect("failed to create renderer");

        let mut ctx = tera::Context::new();
        ctx.insert("content", "testing");

        let rendered = template_renderer
            .one_off("data: {{content}}", &ctx)
            .unwrap();

        assert_eq!(rendered.as_str(), "data: testing");
    }
}
