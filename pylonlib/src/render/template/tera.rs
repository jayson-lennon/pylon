use eyre::WrapErr;
use parking_lot::Mutex;

use std::sync::Arc;
use tera::Tera;
use typed_path::RelPath;

use crate::core::engine::GlobalEnginePaths;
use crate::Result;

use super::TemplateName;

mod functions;

#[derive(Debug)]
pub struct TeraRenderer {
    renderer: Arc<Mutex<Tera>>,
}

impl TeraRenderer {
    pub fn new(engine_paths: GlobalEnginePaths) -> Result<Self> {
        let root = engine_paths
            .absolute_template_dir()
            .join(&RelPath::from_relative("**/*.tera"));

        let mut tera = Tera::new(root.display().to_string().as_str())
            .with_context(|| "error initializing template rendering engine")?;

        register_builtin_functions(engine_paths, &mut tera);

        Ok(Self {
            renderer: Arc::new(Mutex::new(tera)),
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

    #[allow(clippy::redundant_closure_for_method_calls)]
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

fn register_builtin_functions(engine_paths: GlobalEnginePaths, tera: &mut Tera) {
    #[allow(clippy::wildcard_imports)]
    use functions::*;

    {
        let include_file = IncludeFile::new(engine_paths.clone());
        tera.register_function(IncludeFile::NAME, include_file);
    }

    {
        let include_cmd = IncludeCmd::new(engine_paths);
        tera.register_function(IncludeCmd::NAME, include_cmd);
    }
}

#[cfg(test)]
mod test {

    #![allow(warnings, unused)]
    use crate::core::engine::EnginePaths;

    use super::*;
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::TempDir;
    use temptree::temptree;
    use tera::Function;
    use typed_path::{AbsPath, RelPath};

    #[test]
    fn renders_with_valid_template() {
        let tree = temptree! {
            templates: {
                "basic.tera": "data: {{content}}"
            }
        };
        let engine_paths = crate::test::default_test_paths(&tree);

        let template_renderer = TeraRenderer::new(engine_paths).expect("failed to create renderer");

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
        let engine_paths = crate::test::default_test_paths(&tree);

        let template_renderer = TeraRenderer::new(engine_paths).expect("failed to create renderer");

        let ctx = tera::Context::new();

        let rendered = template_renderer.render(&"basic.tera".into(), &ctx);

        assert!(rendered.is_err());
    }

    #[test]
    fn renders_one_off() {
        let tree = temptree! {
            templates: { }
        };
        let engine_paths = crate::test::default_test_paths(&tree);

        let template_renderer = TeraRenderer::new(engine_paths).expect("failed to create renderer");

        let mut ctx = tera::Context::new();
        ctx.insert("content", "testing");

        let rendered = template_renderer
            .one_off("data: {{content}}", &ctx)
            .unwrap();

        assert_eq!(rendered.as_str(), "data: testing");
    }
}
