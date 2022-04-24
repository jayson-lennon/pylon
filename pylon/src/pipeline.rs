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
}

impl Pipeline {
    #[instrument(skip(target_glob))]
    pub fn new<G: TryInto<Glob, Error = globset::Error>>(target_glob: G) -> Result<Self> {
        let target_glob = target_glob.try_into()?;

        trace!("make new pipeline using glob target {}", target_glob.glob());

        Ok(Self {
            target_glob,
            ops: vec![],
        })
    }

    #[instrument(skip(target_glob))]
    pub fn with_ops<G: TryInto<Glob, Error = globset::Error>>(
        target_glob: G,
        ops: &[Operation],
    ) -> Result<Self> {
        let target_glob = target_glob.try_into()?;

        trace!("make new pipeline using glob target {}", target_glob.glob());

        Ok(Self {
            target_glob,
            ops: ops.into(),
        })
    }

    pub fn push_op(&mut self, op: Operation) {
        self.ops.push(op);
    }

    pub fn is_match<P: AsRef<Path>>(&self, asset: P) -> bool {
        self.target_glob.is_match(asset)
    }

    #[instrument(skip(self))]
    pub fn run<S, O, T>(&self, src_root: S, output_root: O, target_asset: T) -> Result<()>
    where
        S: AsRef<Path> + std::fmt::Debug,
        O: AsRef<Path> + std::fmt::Debug,
        T: AsRef<Path> + std::fmt::Debug,
    {
        let src_root = src_root.as_ref();
        let output_root = output_root.as_ref();
        let target_asset = target_asset.as_ref();

        let mut tmp_files = vec![];

        let target_path = {
            let mut buf = PathBuf::from(output_root);
            buf.push(target_asset);
            buf
        };

        let src_path = {
            let mut buf = PathBuf::from(src_root);
            buf.push(target_asset);
            buf
        };

        let mut scratch_path = {
            let scratch_path = new_scratch_file("")?;
            tmp_files.push(scratch_path.clone());
            scratch_path
        };

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
                    autocopy = true;
                    let command = {
                        command
                            .0
                            .replace("$SOURCE", src_path.to_string_lossy().as_ref())
                            .replace("$SCRATCH", scratch_path.to_string_lossy().as_ref())
                            .replace("$TARGET", target_path.to_string_lossy().as_ref())
                    };

                    if command.contains("$NEW_SCRATCH") {
                        scratch_path = new_scratch_file(&std::fs::read_to_string(&scratch_path)?)
                            .with_context(|| {
                            "failed to create new scratch file for shell operation"
                        })?;
                        tmp_files.push(scratch_path.clone());
                    }

                    let command =
                        command.replace("$NEW_SCRATCH", scratch_path.to_string_lossy().as_ref());

                    {
                        let output = std::process::Command::new("sh")
                            .arg("-c")
                            .arg(&command)
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

        clean_temp_files(&tmp_files).with_context(|| "failed to cleanup pipeline scratch files")?;

        Ok(())
    }
}

#[instrument(skip_all)]
fn new_scratch_file(content: &str) -> Result<PathBuf> {
    let tmp = crate::util::gen_temp_file()
        .with_context(|| "Failed to generate temp file for pipeline shell operation".to_string())?
        .path()
        .to_path_buf();
    std::fs::write(&tmp, content.as_bytes())
        .with_context(|| "failed to write contents into scratch file")?;
    Ok(tmp)
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

    use super::{Operation, Pipeline, ShellCommand};
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    fn gen_file_path(dir: &Path, name: &str) -> PathBuf {
        let mut path = PathBuf::from(dir);
        path.push(name);

        path
    }

    #[test]
    fn new_with_ops() {
        let ops = vec![Operation::Copy];

        let pipeline = Pipeline::with_ops("*.txt", ops.as_slice());
        assert!(pipeline.is_ok());
    }

    #[test]
    fn is_match() {
        let mut pipeline = Pipeline::new("*.txt").unwrap();
        pipeline.push_op(Operation::Copy);

        assert_eq!(pipeline.is_match("test.txt"), true);

        assert_eq!(pipeline.is_match("test.md"), false);
    }

    #[test]
    fn op_copy() {
        let mut pipeline = Pipeline::new("*.txt").unwrap();
        pipeline.push_op(Operation::Copy);

        let src_root = tempdir().unwrap();
        let output_root = tempdir().unwrap();
        let target_asset = "test.txt";

        let src_path = gen_file_path(src_root.path(), "test.txt");
        fs::write(&src_path, b"test data").unwrap();

        pipeline
            .run(src_root.path(), output_root.path(), target_asset)
            .unwrap();

        let target_path = gen_file_path(output_root.path(), "test.txt");
        assert!(target_path.exists());

        let target_content = fs::read_to_string(target_path).unwrap();
        assert_eq!(&target_content, "test data");
    }

    #[test]
    fn multiple_shell_ops() {
        let mut pipeline = Pipeline::new("*.txt").unwrap();
        pipeline.push_op(Operation::Shell(ShellCommand::new(
            r#"sed 's/old/new/g' $SOURCE > $NEW_SCRATCH"#,
        )));
        pipeline.push_op(Operation::Shell(ShellCommand::new(
            r#"sed 's/new/hot/g' $SCRATCH > $NEW_SCRATCH"#,
        )));

        let src_root = tempdir().unwrap();
        let output_root = tempdir().unwrap();
        let target_asset = "test.txt";

        let src_path = gen_file_path(src_root.path(), "test.txt");
        fs::write(&src_path, b"old").unwrap();

        pipeline
            .run(src_root.path(), output_root.path(), target_asset)
            .unwrap();

        let target_path = gen_file_path(output_root.path(), "test.txt");
        assert!(target_path.exists());

        let target_content = fs::read_to_string(target_path).unwrap();
        assert_eq!(&target_content, "hot");
    }

    #[test]
    fn multiple_shell_ops_with_new_scratches() {
        let mut pipeline = Pipeline::new("*.txt").unwrap();
        pipeline.push_op(Operation::Shell(ShellCommand::new(
            r#"cat $SOURCE > $NEW_SCRATCH"#,
        )));
        pipeline.push_op(Operation::Shell(ShellCommand::new(
            r#"echo test >> $NEW_SCRATCH"#,
        )));
        pipeline.push_op(Operation::Shell(ShellCommand::new(
            r#"echo test >> $NEW_SCRATCH"#,
        )));

        let src_root = tempdir().unwrap();
        let output_root = tempdir().unwrap();
        let target_asset = "test.txt";

        let src_path = gen_file_path(src_root.path(), "test.txt");
        fs::write(&src_path, b"initial").unwrap();

        pipeline
            .run(src_root.path(), output_root.path(), target_asset)
            .unwrap();

        let target_path = gen_file_path(output_root.path(), "test.txt");
        assert!(target_path.exists());

        let target_content = fs::read_to_string(target_path).unwrap();
        assert_eq!(&target_content, "initialtest\ntest\n");
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
        let mut pipeline = Pipeline::new("*.txt").unwrap();
        pipeline.push_op(Operation::Shell(ShellCommand::new("__COMMAND_NOT_FOUND__")));

        let src_root = tempdir().unwrap();
        let output_root = tempdir().unwrap();
        let target_asset = "test.txt";

        let src_path = gen_file_path(src_root.path(), "test.txt");
        fs::write(&src_path, b"test data").unwrap();

        let result = pipeline.run(src_root.path(), output_root.path(), target_asset);

        assert!(result.is_err());
    }
}
