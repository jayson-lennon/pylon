use crate::Result;
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
    pub fn new<P: AsRef<Path>>(template_root: P, syntax_theme_root: P) -> Result<Self> {
        let tera = template::TeraRenderer::new(template_root)?;
        let markdown = markup::MarkdownRenderer::new();
        let highlight = highlight::SyntectHighlighter::new(syntax_theme_root)?;
        Ok(Self {
            tera,
            markdown,
            highlight,
        })
    }
}
