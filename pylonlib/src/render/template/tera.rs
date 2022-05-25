use eyre::{WrapErr};
use parking_lot::Mutex;

use std::sync::Arc;
use tera::Tera;
use typed_path::RelPath;

use crate::core::engine::GlobalEnginePaths;
use crate::Result;

use super::TemplateName;

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

    let include_file = IncludeFile::new(engine_paths);
    tera.register_function(IncludeFile::NAME, include_file);
}

mod functions {
    use std::collections::HashMap;

    use typed_path::{AbsPath};

    use crate::core::engine::GlobalEnginePaths;

    macro_rules! tera_error {
        ($msg:expr) => {{
            |_| tera::Error::msg($msg)
        }};
    }

    pub struct IncludeFile {
        engine_paths: GlobalEnginePaths,
    }

    impl IncludeFile {
        pub const NAME: &'static str = "include_file";

        pub fn new(engine_paths: GlobalEnginePaths) -> Self {
            Self { engine_paths }
        }
    }

    impl tera::Function for IncludeFile {
        fn call(&self, args: &HashMap<String, tera::Value>) -> tera::Result<tera::Value> {
            let path = {
                let path: &str = args
                    .get("path")
                    .ok_or_else(|| tera::Error::msg("`path` required to include file in template"))?
                    .as_str()
                    .ok_or_else(|| {
                        format!(
                            "failed to interpret path '{}' as a string",
                            args.get("path").unwrap(),
                        )
                    })?;

                let relative_to_project_root = AbsPath::new(path)
                    .map_err(tera_error!(
                        "'path' must be absolute (starting from project directory)"
                    ))?
                    .strip_prefix("/")
                    .expect("absolute path must start with a '/'");

                self.engine_paths
                    .project_root()
                    .join(&relative_to_project_root)
            };

            let content = std::fs::read_to_string(&path).map_err(|e| {
                tera::Error::msg(format!("error reading file at '{}': {}", path, e))
            })?;

            Ok(tera::Value::String(content))
        }

        fn is_safe(&self) -> bool {
            true
        }
    }

    #[cfg(test)]
    mod test {
        

        use super::*;
        use serde_json::json;
        
        
        use temptree::temptree;
        use tera::Function;

        #[test]
        fn include_file_happy_path() {
            let tree = temptree! {
                src: {
                    "file.ext": "content",
                }
            };

            let engine_paths = crate::test::default_test_paths(&tree);

            let include_file = IncludeFile::new(engine_paths);

            let mut args = HashMap::new();
            args.insert("path".to_owned(), json!("/src/file.ext"));

            let result = include_file.call(&args).expect("call should be successful");
            assert_eq!(result, "content");
        }

        #[test]
        fn include_file_fails_with_relative_path() {
            let tree = temptree! {};

            let engine_paths = crate::test::default_test_paths(&tree);

            let include_file = IncludeFile::new(engine_paths);

            let mut args = HashMap::new();
            args.insert("path".to_owned(), json!("src/file.ext"));

            let result = include_file.call(&args);
            assert!(result.is_err());
        }

        #[test]
        fn include_file_fails_when_missing_file() {
            let tree = temptree! {};

            let engine_paths = crate::test::default_test_paths(&tree);

            let include_file = IncludeFile::new(engine_paths);

            let mut args = HashMap::new();
            args.insert("path".to_owned(), json!("/src/missing.ext"));

            let result = include_file.call(&args);
            assert!(result.is_err());
        }

        #[test]
        fn include_file_fails_when_targeting_directory() {
            let tree = temptree! {
                src: {}
            };

            let engine_paths = crate::test::default_test_paths(&tree);

            let include_file = IncludeFile::new(engine_paths);

            let mut args = HashMap::new();
            args.insert("path".to_owned(), json!("/src"));

            let result = include_file.call(&args);
            assert!(result.is_err());
        }

        #[test]
        fn include_file_fails_when_path_is_not_a_string() {
            let tree = temptree! {
                src: {}
            };

            let engine_paths = crate::test::default_test_paths(&tree);

            let include_file = IncludeFile::new(engine_paths);

            let mut args = HashMap::new();
            args.insert("path".to_owned(), json!(1));

            let result = include_file.call(&args);
            assert!(result.is_err());
        }

        #[test]
        fn name() {
            assert_eq!(IncludeFile::NAME, "include_file");
        }
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
