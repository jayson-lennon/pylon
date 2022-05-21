use crate::Result;
use eyre::WrapErr;
use std::path::Path;

pub mod highlight;
pub mod markup;
pub mod shortcode;
pub mod template;

#[derive(Debug)]
pub struct Renderers {
    tera: template::TeraRenderer,
    markdown: markup::MarkdownRenderer,
    highlight: highlight::SyntectHighlighter,
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

    pub fn markdown(&self) -> &markup::MarkdownRenderer {
        &self.markdown
    }

    pub fn highlight(&self) -> &highlight::SyntectHighlighter {
        &self.highlight
    }

    pub fn tera(&self) -> &template::TeraRenderer {
        &self.tera
    }

    pub fn tera_mut(&mut self) -> &mut template::TeraRenderer {
        &mut self.tera
    }
}
