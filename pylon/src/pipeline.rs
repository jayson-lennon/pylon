use crate::core::SysPath;
use crate::util::Glob;
use crate::Result;
use anyhow::{anyhow, Context};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::str::FromStr;
use tracing::{error, info_span, instrument, trace, trace_span};

#[derive(Clone, Debug)]
pub struct ShellCommand(String);

impl ShellCommand {
    pub fn new<T: AsRef<str>>(cmd: T) -> Self {
        Self(cmd.as_ref().to_string())
    }
}

#[derive(Clone, Debug)]
pub enum Operation {
    Copy,
    Shell(ShellCommand),
}

impl FromStr for Operation {
    type Err = &'static str;

    #[instrument(ret)]
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "[COPY]" => Ok(Self::Copy),
            other => Ok(Self::Shell(ShellCommand(other.to_owned()))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Pipeline {
    pub target_glob: Glob,
    ops: Vec<Operation>,
    base_dir: PathBuf,
    project_root: PathBuf,
}

impl Pipeline {
    #[instrument(skip(target_glob))]
    pub fn new<R, B, G>(project_root: R, base_dir: B, target_glob: G) -> Result<Self>
    where
        R: Into<PathBuf> + std::fmt::Debug,
        B: Into<PathBuf> + std::fmt::Debug,
        G: TryInto<Glob, Error = globset::Error>,
    {
        let target_glob = target_glob.try_into()?;

        trace!("make new pipeline using glob target {}", target_glob.glob());

        Ok(Self {
            target_glob,
            ops: vec![],
            base_dir: base_dir.into(),
            project_root: project_root.into(),
        })
    }

    #[instrument(skip(target_glob))]
    pub fn with_ops<R, B, G>(
        project_root: R,
        base_dir: B,
        target_glob: G,
        ops: &[Operation],
    ) -> Result<Self>
    where
        R: Into<PathBuf> + std::fmt::Debug,
        B: Into<PathBuf> + std::fmt::Debug,
        G: TryInto<Glob, Error = globset::Error>,
    {
        let target_glob = target_glob.try_into()?;

        trace!("make new pipeline using glob target {}", target_glob.glob());

        Ok(Self {
            target_glob,
            ops: ops.into(),
            base_dir: base_dir.into(),
            project_root: project_root.into(),
        })
    }

    pub fn push_op(&mut self, op: Operation) {
        self.ops.push(op);
    }

    pub fn is_match<P: AsRef<Path>>(&self, asset: P) -> bool {
        self.target_glob.is_match(asset)
    }

    #[instrument(skip(self))]
    pub fn run<O, T, W>(&self, output_root: O, target_asset: T, working_dir: W) -> Result<()>
    where
        O: AsRef<Path> + std::fmt::Debug,
        T: AsRef<Path> + std::fmt::Debug,
        W: AsRef<Path> + std::fmt::Debug,
    {
        let mut scratch_files = vec![];
        let result = self.do_run(&mut scratch_files, output_root, target_asset, working_dir);

        clean_temp_files(&scratch_files)
            .with_context(|| "failed to cleanup pipeline scratch files")?;

        result
    }

    #[allow(clippy::too_many_lines)]
    fn do_run<O, T, W>(
        &self,
        scratch_files: &mut Vec<PathBuf>,
        output_root: O,
        target_asset: T,
        working_dir: W,
    ) -> Result<()>
    where
        O: AsRef<Path> + std::fmt::Debug,
        T: AsRef<Path> + std::fmt::Debug,
        W: AsRef<Path> + std::fmt::Debug,
    {
        let output_root = output_root.as_ref();
        let target_asset = target_asset.as_ref();

        let target_path = {
            let mut buf = PathBuf::from(output_root);
            buf.push(target_asset);
            buf
        };

        let src_path = {
            let mut buf = self.base_dir.clone();
            buf.push(target_asset);
            buf
        };

        let mut scratch_path = new_scratch_file(scratch_files, &[])?;

        let mut autocopy = false;

        for op in &self.ops {
            let _span = info_span!("perform pipeline operation").entered();
            match op {
                Operation::Copy => {
                    trace!("copy: {:?} -> {:?}", src_path, target_path);
                    std::fs::copy(&src_path, &target_path).with_context(||format!("Failed performing copy operation in pipeline. '{src_path:?}' -> '{target_path:?}'"))?;
                }
                Operation::Shell(command) => {
                    trace!("shell command: {:?}", command);
                    if command.0.contains("$TARGET") {
                        autocopy = false;
                    } else {
                        autocopy = true;
                    }

                    let source = {
                        if let Ok(relative_to_project_root) = self.base_dir.strip_prefix("/") {
                            let mut source = self.project_root.clone();
                            source.push(relative_to_project_root);
                            source.push(target_asset);
                            source.to_string_lossy().to_string()
                        } else {
                            src_path.to_string_lossy().to_string()
                        }
                    };
                    trace!("source {:?}", source);

                    let command = {
                        command
                            .0
                            .replace("$SOURCE", &source)
                            .replace("$SCRATCH", scratch_path.to_string_lossy().as_ref())
                            .replace("$TARGET", target_path.to_string_lossy().as_ref())
                    };

                    if command.contains("$NEW_SCRATCH") {
                        eprintln!("make new scratch file");
                        scratch_path =
                            new_scratch_file(scratch_files, &std::fs::read(&scratch_path)?)
                                .with_context(|| {
                                    "failed to create new scratch file for shell operation"
                                })?;
                    }

                    let command =
                        command.replace("$NEW_SCRATCH", scratch_path.to_string_lossy().as_ref());

                    trace!("command= {:?}", command);
                    {
                        let working_dir = {
                            if let Ok(relative_to_project_root) = self.base_dir.strip_prefix("/") {
                                let mut working_dir = self.project_root.clone();
                                working_dir.push(relative_to_project_root);
                                working_dir
                            } else {
                                let mut new_working_dir = self.project_root.clone();
                                new_working_dir.push(self.base_dir.clone());
                                new_working_dir.push(working_dir.as_ref());
                                new_working_dir
                            }
                        };
                        let cmd = format!("cd {} &&  {}", working_dir.to_string_lossy(), &command);
                        trace!("cmd= {:?}", cmd);
                        dbg!(&cmd);

                        let output = std::process::Command::new("sh")
                            .arg("-c")
                            .arg(&cmd)
                            .stdout(Stdio::piped())
                            .stderr(Stdio::piped())
                            .output()
                            .with_context(|| {
                                format!("Failed running shell pipeline command: '{command}'")
                            })?;
                        if !output.status.success() {
                            let stdout = String::from_utf8_lossy(&output.stdout);
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            error!(
                                command = %command,
                                stderr = %stderr,
                                stdout = %stdout,
                                "Pipeline command failed"
                            );
                            return Err(anyhow!("pipeline processing failure"));
                        }
                    }
                }
            }
        }

        if autocopy {
            std::fs::copy(&scratch_path, &target_path).with_context(||format!("Failed performing copy operation in pipeline. '{scratch_path:?}' -> '{target_path:?}'"))?;
        }

        Ok(())
    }
}

#[instrument(skip_all)]
fn new_scratch_file(files: &mut Vec<PathBuf>, content: &[u8]) -> Result<PathBuf> {
    let tmp = crate::util::gen_temp_file()
        .with_context(|| "Failed to generate temp file for pipeline shell operation")?
        .path()
        .to_path_buf();
    files.push(tmp.clone());
    std::fs::write(&tmp, content).with_context(|| "failed to write contents into scratch file")?;
    Ok(files[files.len() - 1].clone())
}

fn clean_temp_files(tmp_files: &[PathBuf]) -> Result<()> {
    let _span = trace_span!("clean up temp files").entered();
    trace!(files = ?tmp_files);
    for f in tmp_files {
        std::fs::remove_file(&f).with_context(|| {
            format!(
                "Failed to clean up temporary file: '{}'",
                f.to_string_lossy()
            )
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod test {
    #![allow(clippy::all)]
    #![allow(clippy::pedantic)]

    use crate::{core::SysPath, util::TMP_ARTIFACT_PREFIX};

    use super::{Operation, Pipeline, ShellCommand};
    use std::fs;
    use temptree::temptree;

    #[test]
    #[ignore]
    fn check_artifacts() {
        let tmp_dir = temptree! {
            "empty": "",
        };
        let tmp_dir = tmp_dir
            .path()
            .parent()
            .expect("temp file should have a parent dir");
        let entries = std::fs::read_dir(tmp_dir).unwrap();
        for entry in entries {
            let entry = entry.unwrap();
            let file_name = entry.file_name();
            let file_name = file_name.to_string_lossy();
            if file_name.starts_with(TMP_ARTIFACT_PREFIX) {
                panic!("leftover artifact: {:?}", entry.path());
            }
        }
    }

    #[test]
    fn new_with_ops() {
        let tmp_dir = temptree! {
            nothing: "",
        };
        let ops = vec![Operation::Copy];

        let pipeline = Pipeline::with_ops(tmp_dir.path(), "base", "*.txt", ops.as_slice());
        assert!(pipeline.is_ok());
    }

    #[test]
    fn is_match() {
        let tmp_dir = temptree! {
            nothing: "",
        };
        let mut pipeline = Pipeline::new(tmp_dir.path(), "base", "*.txt").unwrap();
        pipeline.push_op(Operation::Copy);

        assert_eq!(pipeline.is_match("test.txt"), true);

        assert_eq!(pipeline.is_match("test.md"), false);
    }

    #[test]
    fn op_copy() {
        let tree = temptree! {
          src: {
              "test.txt": "data",
              "sample.md": "",
          },
          target: {},
        };

        let mut pipeline = Pipeline::new(tree.path(), tree.path().join("src"), "*.txt").unwrap();
        pipeline.push_op(Operation::Copy);

        pipeline
            .run(
                tree.path().join("target"),
                "test.txt",
                tree.path().join("src"),
            )
            .unwrap();

        let target_content = fs::read_to_string(tree.path().join("target/test.txt")).unwrap();
        assert_eq!(&target_content, "data");
    }

    #[test]
    fn multiple_shell_ops() {
        let tree = temptree! {
          src: {
              "test.txt": "old",
              "sample.md": "",
          },
          target: {},
        };
        let mut pipeline = Pipeline::new(tree.path(), ".", "**/*.txt").unwrap();
        pipeline.push_op(Operation::Shell(ShellCommand::new(
            r#"sed 's/old/new/g' $SOURCE > $NEW_SCRATCH"#,
        )));
        pipeline.push_op(Operation::Shell(ShellCommand::new(
            r#"sed 's/new/hot/g' $SCRATCH > $NEW_SCRATCH"#,
        )));

        pipeline
            .run(
                tree.path().join("target"),
                "test.txt",
                tree.path().join("src"),
            )
            .unwrap();

        let target_content = fs::read_to_string(tree.path().join("target/test.txt")).unwrap();
        assert_eq!(&target_content, "hot");
    }

    #[test]
    fn multiple_shell_ops_autocopy_disabled() {
        let tree = temptree! {
          src: {
              "test.txt": "old",
              "sample.md": "",
          },
          target: {},
        };
        let mut pipeline = Pipeline::new(tree.path(), ".", "*.txt").unwrap();
        pipeline.push_op(Operation::Shell(ShellCommand::new(
            r#"sed 's/old/new/g' $SOURCE > $NEW_SCRATCH"#,
        )));
        pipeline.push_op(Operation::Shell(ShellCommand::new(
            r#"sed 's/new/hot/g' $SCRATCH > $TARGET"#,
        )));

        pipeline
            .run(
                tree.path().join("target"),
                "test.txt",
                tree.path().join("src"),
            )
            .unwrap();

        let target_content = fs::read_to_string(tree.path().join("target/test.txt")).unwrap();
        assert_eq!(&target_content, "hot");
    }

    #[test]
    fn operation_fromstr_impl_copy() {
        use std::str::FromStr;

        let operation = Operation::from_str("[COPY]").unwrap();
        match operation {
            Operation::Copy => (),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn operation_fromstr_impl_shell() {
        use std::str::FromStr;

        let operation = Operation::from_str("echo hello").unwrap();
        match operation {
            Operation::Shell(_) => (),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn handles_broken_shell_op() {
        let tree = temptree! {
          src: {
              "test.txt": "data",
          },
          target: {},
        };

        let mut pipeline = Pipeline::new(tree.path(), tree.path().join("src"), "*.txt").unwrap();
        pipeline.push_op(Operation::Shell(ShellCommand::new("__COMMAND_NOT_FOUND__")));

        let result = pipeline.run(
            tree.path().join("target"),
            "test.txt",
            tree.path().join("src"),
        );

        assert!(result.is_err());
    }

    #[test]
    fn use_relative_base_dir_with_dot_only() {
        let tree = temptree! {
          src: {
              blog: {
                  post1: {
                    "sample.md": "",
                    "asset.txt": "old1",
                    "asset2.txt": "old2",
                  }
              }
          },
          target: {},
          "ignore.txt": "ignore"
        };
        let mut pipeline = Pipeline::new(tree.path(), ".", "**/blog/**/*.txt").unwrap();
        pipeline.push_op(Operation::Shell(ShellCommand::new(
            r#"sed 's/old/new/g' $SOURCE > $TARGET"#,
        )));

        pipeline
            .run(
                tree.path().join("target"),
                "asset.txt",
                tree.path().join("src/blog/post1"),
            )
            .unwrap();

        pipeline
            .run(
                tree.path().join("target"),
                "asset2.txt",
                tree.path().join("src/blog/post1"),
            )
            .unwrap();

        let target_content = fs::read_to_string(tree.path().join("target/asset.txt")).unwrap();
        assert_eq!(&target_content, "new1");

        let target_content = fs::read_to_string(tree.path().join("target/asset2.txt")).unwrap();
        assert_eq!(&target_content, "new2");
    }

    #[test]
    fn use_relative_base_dir_with_dot() {
        let tree = temptree! {
          src: {
              blog: {
                  post1: {
                    "sample.md": "",
                    data: {
                        "asset.txt": "old1",
                        "asset2.txt": "old2",
                    }
                  }
              }
          },
          target: {},
          "ignore.txt": "ignore"
        };
        let mut pipeline = Pipeline::new(tree.path(), "./data", "**/blog/**/*.txt").unwrap();
        pipeline.push_op(Operation::Shell(ShellCommand::new(
            r#"sed 's/old/new/g' $SOURCE > $TARGET"#,
        )));

        pipeline
            .run(
                tree.path().join("target"),
                "asset.txt",
                tree.path().join("src/blog/post1"),
            )
            .unwrap();

        pipeline
            .run(
                tree.path().join("target"),
                "asset2.txt",
                tree.path().join("src/blog/post1"),
            )
            .unwrap();

        let target_content = fs::read_to_string(tree.path().join("target/asset.txt")).unwrap();
        assert_eq!(&target_content, "new1");

        let target_content = fs::read_to_string(tree.path().join("target/asset2.txt")).unwrap();
        assert_eq!(&target_content, "new2");
    }

    #[test]
    fn use_relative_base_dir() {
        let tree = temptree! {
          src: {
              blog: {
                  post1: {
                    "sample.md": "",
                    data: {
                        "asset.txt": "old1",
                        "asset2.txt": "old2",
                    }
                  }
              }
          },
          target: {},
          "ignore.txt": "ignore"
        };
        let mut pipeline = Pipeline::new(tree.path(), "data", "**/blog/**/*.txt").unwrap();
        pipeline.push_op(Operation::Shell(ShellCommand::new(
            r#"sed 's/old/new/g' $SOURCE > $TARGET"#,
        )));

        pipeline
            .run(
                tree.path().join("target"),
                "asset.txt",
                tree.path().join("src/blog/post1"),
            )
            .unwrap();

        pipeline
            .run(
                tree.path().join("target"),
                "asset2.txt",
                tree.path().join("src/blog/post1"),
            )
            .unwrap();

        let target_content = fs::read_to_string(tree.path().join("target/asset.txt")).unwrap();
        assert_eq!(&target_content, "new1");

        let target_content = fs::read_to_string(tree.path().join("target/asset2.txt")).unwrap();
        assert_eq!(&target_content, "new2");
    }

    #[test]
    fn use_absolute_base_dir() {
        let tree = temptree! {
          src: {
              blog: {
                  post1: {
                    "sample.md": "",
                    data: {
                        "asset.txt": "old1",
                        "asset2.txt": "old2",
                    }
                  }
              }
          },
          target: {},
          "ignore.txt": "ignore"
        };
        let mut pipeline =
            Pipeline::new(tree.path(), "/src/blog/post1/data", "**/blog/**/*.txt").unwrap();
        pipeline.push_op(Operation::Shell(ShellCommand::new(
            r#"sed 's/old/new/g' $SOURCE > $TARGET"#,
        )));

        pipeline
            .run(
                tree.path().join("target"),
                "asset.txt",
                tree.path().join("src/blog/post1"),
            )
            .unwrap();

        pipeline
            .run(
                tree.path().join("target"),
                "asset2.txt",
                tree.path().join("src/blog/post1"),
            )
            .unwrap();

        let target_content = fs::read_to_string(tree.path().join("target/asset.txt")).unwrap();
        assert_eq!(&target_content, "new1");

        let target_content = fs::read_to_string(tree.path().join("target/asset2.txt")).unwrap();
        assert_eq!(&target_content, "new2");
    }

    #[test]
    fn use_root() {
        let tree = temptree! {
          src: {
              blog: {
                  post1: {
                    "sample.md": "",
                  }
              }
          },
          target: {},
          "ignore.txt": "ignore",
          "asset.txt": "old1",
          "asset2.txt": "old2",
        };
        let mut pipeline = Pipeline::new(tree.path(), "/", "*.txt").unwrap();
        pipeline.push_op(Operation::Shell(ShellCommand::new(
            r#"sed 's/old/new/g' $SOURCE > $TARGET"#,
        )));

        pipeline
            .run(
                tree.path().join("target"),
                "asset.txt",
                tree.path().join("src/blog/post1"),
            )
            .unwrap();

        pipeline
            .run(
                tree.path().join("target"),
                "asset2.txt",
                tree.path().join("src/blog/post1"),
            )
            .unwrap();

        let target_content = fs::read_to_string(tree.path().join("target/asset.txt")).unwrap();
        assert_eq!(&target_content, "new1");

        let target_content = fs::read_to_string(tree.path().join("target/asset2.txt")).unwrap();
        assert_eq!(&target_content, "new2");
    }

    #[test]
    fn bugfix_realworld_pipeline_processing() {
        let tree = temptree! {
            content: {
                blog: {
                    some_post: {
                        "index.md": "",
                        _src: {
                            "sample1.txt": "sample1",
                            "sample2.txt": "sample2",
                        }
                    }
                }
            },
            public: {}
        };
        let mut pipeline = Pipeline::new(tree.path(), ".", "**/blog/**/combined.txt").unwrap();
        pipeline.push_op(Operation::Shell(ShellCommand::new("cat _src/* > $TARGET")));

        pipeline
            .run(
                tree.path().join("public"),
                "combined.txt",
                tree.path().join("content/blog/some_post"),
            )
            .unwrap();

        let target_content = fs::read_to_string(tree.path().join("public/combined.txt")).unwrap();
        assert_eq!(&target_content, "sample1sample2");
    }
}
