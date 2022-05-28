use std::collections::HashMap;

use typed_path::AbsPath;

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

        let content = std::fs::read_to_string(&path)
            .map_err(|e| tera::Error::msg(format!("error reading file at '{}': {}", path, e)))?;

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
