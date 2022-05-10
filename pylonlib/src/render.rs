use crate::Result;
use eyre::WrapErr;
use std::path::Path;

pub mod highlight;
pub mod markup;
pub mod template;

#[derive(Debug)]
pub struct Renderers {
    pub tera: template::TeraRenderer,
    pub markdown: markup::MarkdownRenderer,
    pub highlight: highlight::SyntectHighlighter,
}

impl Renderers {
    pub fn new<P: AsRef<Path>>(template_root: P) -> Result<Self> {
        let template_root = template_root.as_ref();
        let tera = template::TeraRenderer::new(template_root).wrap_err_with(|| {
            format!(
                "Failed to initialize Tera with template root of '{}'",
                template_root.display()
            )
        })?;
        let markdown = markup::MarkdownRenderer::new();
        let highlight =
            highlight::SyntectHighlighter::new().wrap_err("Failed to initialize Syntect")?;
        Ok(Self {
            tera,
            markdown,
            highlight,
        })
    }
}
