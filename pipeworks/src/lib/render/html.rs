use std::path::Path;
use tera::Tera;

pub trait HtmlRenderer {
    type Context;
    type RenderingError;
    fn render<T: AsRef<str>>(
        &self,
        template: T,
        context: &Self::Context,
    ) -> Result<String, Self::RenderingError>;
}

pub struct TeraRenderer {
    renderer: Tera,
}

impl TeraRenderer {
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        let r = Tera::new(
            root.as_ref()
                .to_str()
                .expect("non UTF-8 characters in path"),
        )
        .expect("error initializing template rendering engine");
        for t in r.get_template_names() {
            dbg!(t);
        }
        Self { renderer: r }
    }
}

impl HtmlRenderer for TeraRenderer {
    type Context = tera::Context;
    type RenderingError = anyhow::Error;

    fn render<T: AsRef<str>>(
        &self,
        template: T,
        context: &Self::Context,
    ) -> Result<String, Self::RenderingError> {
        Ok(self.renderer.render(template.as_ref(), context)?)
    }
}
