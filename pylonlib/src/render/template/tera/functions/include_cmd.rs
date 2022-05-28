use std::collections::HashMap;
use tap::Pipe;


use typed_path::{AbsPath, RelPath};

use crate::core::engine::GlobalEnginePaths;

macro_rules! tera_error {
    ($msg:expr) => {{
        |_| tera::Error::msg($msg)
    }};
}

#[derive(Clone, Debug)]
pub struct IncludeCmd {
    engine_paths: GlobalEnginePaths,
}

impl IncludeCmd {
    pub const NAME: &'static str = "include_cmd";

    pub fn new(engine_paths: GlobalEnginePaths) -> Self {
        Self { engine_paths }
    }
}

impl tera::Function for IncludeCmd {
    fn call(&self, args: &HashMap<String, tera::Value>) -> tera::Result<tera::Value> {
        let working_dir = {
            let working_dir: AbsPath = args
                .get("cwd")
                .ok_or_else(|| tera::Error::msg("`cwd` required to include cmd in template"))?
                .as_str()
                .ok_or_else(|| {
                    format!(
                        "failed to interpret cwd '{}' as a string",
                        args.get("cwd").unwrap(),
                    )
                })?
                .pipe(AbsPath::new)
                .map_err(|e| {
                    tera::Error::msg(format!(
                        "`cwd` must be absolute (start with `/` from project root): {e}"
                    ))
                })?;

            // take the project root and concat with the `cwd` provided in the args
            self.engine_paths
                .project_root()
                .join(&RelPath::from_relative(
                    // always works, validated above
                    &working_dir.strip_prefix("/").unwrap(),
                ))
        };

        let command: &str = {
            args.get("cmd")
                .ok_or_else(|| tera::Error::msg("`cmd` required to include cmd in template"))?
                .as_str()
                .ok_or_else(|| {
                    format!(
                        "failed to interpret cmd '{}' as a string",
                        args.get("cmd").unwrap(),
                    )
                })?
        };

        let content = pipeworks::run_command(command, &working_dir)
            .map_err(|e| tera::Error::msg(format!("failed to run command '{command}': {e}")))?;

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
    fn include_cmd_happy_path() {
        let tree = temptree! {};

        let engine_paths = crate::test::default_test_paths(&tree);

        let include_cmd = IncludeCmd::new(engine_paths);

        let mut args = HashMap::new();
        args.insert("cwd".to_owned(), json!(format!("{}", "/")));
        args.insert("cmd".to_owned(), json!("echo hello"));

        let result = include_cmd.call(&args).expect("call should be successful");
        assert_eq!(result, "hello\n");
    }

    #[test]
    fn include_cmd_cat_file() {
        let tree = temptree! {
            src: {
                "file.ext": "content",
            }
        };

        let engine_paths = crate::test::default_test_paths(&tree);

        let include_cmd = IncludeCmd::new(engine_paths);

        let mut args = HashMap::new();
        args.insert("cwd".to_owned(), json!(format!("{}", "/")));
        args.insert("cmd".to_owned(), json!("cat src/file.ext"));

        let result = include_cmd.call(&args).expect("call should be successful");
        assert_eq!(result, "content");
    }

    #[test]
    fn include_cmd_composite() {
        let tree = temptree! {
            src: {
                "file.ext": "",
            }
        };

        let engine_paths = crate::test::default_test_paths(&tree);

        let include_cmd = IncludeCmd::new(engine_paths);

        let mut args = HashMap::new();
        args.insert("cwd".to_owned(), json!(format!("{}", "/")));
        args.insert(
            "cmd".to_owned(),
            json!("echo test > src/file.ext && cat src/file.ext"),
        );

        let result = include_cmd.call(&args).expect("call should be successful");
        assert_eq!(result, "test\n");
    }

    #[test]
    fn include_cmd_fails_with_invalid_command() {
        let tree = temptree! {};

        let engine_paths = crate::test::default_test_paths(&tree);

        let include_cmd = IncludeCmd::new(engine_paths);

        let mut args = HashMap::new();
        args.insert("cwd".to_owned(), json!(format!("{}", "/")));
        args.insert("cmd".to_owned(), json!("COMMAND_NOT_FOUND"));

        let result = include_cmd.call(&args);
        assert!(result.is_err());
    }

    #[test]
    fn include_cmd_fails_when_missing_cmd() {
        let tree = temptree! {};

        let engine_paths = crate::test::default_test_paths(&tree);

        let include_cmd = IncludeCmd::new(engine_paths);

        let mut args = HashMap::new();
        args.insert("cwd".to_owned(), json!(format!("{}", "/")));

        let result = include_cmd.call(&args);
        assert!(result.is_err());
    }

    #[test]
    fn include_cmd_fails_when_missing_cwd() {
        let tree = temptree! {};

        let engine_paths = crate::test::default_test_paths(&tree);

        let include_cmd = IncludeCmd::new(engine_paths);

        let mut args = HashMap::new();
        args.insert("cmd".to_owned(), json!("COMMAND_NOT_FOUND"));

        let result = include_cmd.call(&args);
        assert!(result.is_err());
    }

    #[test]
    fn include_cmd_fails_when_missing_cwd_and_cmd() {
        let tree = temptree! {};

        let engine_paths = crate::test::default_test_paths(&tree);

        let include_cmd = IncludeCmd::new(engine_paths);

        let args = HashMap::new();

        let result = include_cmd.call(&args);
        assert!(result.is_err());
    }
}
