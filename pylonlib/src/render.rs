use crate::{core::engine::GlobalEnginePaths, Result};
use eyre::WrapErr;
use std::path::Path;

pub mod highlight;
pub mod markup;
pub mod template;

#[derive(Debug)]
pub struct Renderers {
    tera: template::TeraRenderer,
    markdown: markup::MarkdownRenderer,
    highlight: highlight::SyntectHighlighter,
    engine_paths: GlobalEnginePaths,
}

impl Renderers {
    pub fn new(engine_paths: GlobalEnginePaths) -> Result<Self> {
        let tera = template::TeraRenderer::new(engine_paths.clone()).wrap_err_with(|| {
            format!(
                "Failed to initialize Tera with template root of '{}'",
                engine_paths.absolute_template_dir().display()
            )
        })?;
        let markdown = markup::MarkdownRenderer::new();
        let highlight =
            highlight::SyntectHighlighter::new().wrap_err("Failed to initialize Syntect")?;
        Ok(Self {
            tera,
            markdown,
            highlight,
            engine_paths,
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
