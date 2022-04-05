use std::path::Path;

pub mod markup;
pub mod rendered_page;
pub mod template;

pub use rendered_page::render as page;

#[derive(Debug)]
pub struct Renderers {
    pub tera: template::TeraRenderer,
    pub markdown: markup::MarkdownRenderer,
}

impl Renderers {
    pub fn new<P: AsRef<Path>>(template_root: P) -> Self {
        let tera = template::TeraRenderer::new(template_root);
        let markdown = markup::MarkdownRenderer::new();
        Self { tera, markdown }
    }
}
