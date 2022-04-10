use crate::util::{Glob, GlobCandidate};
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

    pub fn has_input(&self) -> bool {
        self.0.contains("$INPUT")
    }

    pub fn has_output(&self) -> bool {
        self.0.contains("$OUTPUT")
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
    fn from_str(s: &str) -> Result<Self, Self::Err> {
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
    pub fn new<G: TryInto<Glob, Error = globset::Error>>(
        target_glob: G,
    ) -> Result<Self, anyhow::Error> {
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
    ) -> Result<Self, anyhow::Error> {
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
    pub fn run<S, O, T>(
        &self,
        src_root: S,
        output_root: O,
        target_asset: T,
    ) -> Result<(), anyhow::Error>
    where
        S: AsRef<Path> + std::fmt::Debug,
        O: AsRef<Path> + std::fmt::Debug,
        T: AsRef<Path> + std::fmt::Debug,
    {
        let src_root = src_root.as_ref();
        let output_root = output_root.as_ref();
        let target_asset = target_asset.as_ref();

        let mut tmp_files = vec![];
        let mut input_path = {
            let mut buf = PathBuf::from(src_root);
            buf.push(target_asset);
            buf
        };

        let output_path = {
            let mut buf = PathBuf::from(output_root);
            buf.push(target_asset);
            buf
        };

        for op in &self.ops {
            let _span = info_span!("perform pipeline operation").entered();
            match op {
                Operation::Copy => {
                    trace!("copy: {:?} -> {:?}", input_path, output_path);
                    std::fs::copy(&input_path, &output_path).with_context(||format!("Failed performing copy operation in pipeline. '{input_path:?}' -> '{output_path:?}'"))?;
                }
                Operation::Shell(command) => {
                    trace!("shell command: {:?}", command);
                    let artifact_path = {
                        if command.has_output() {
                            let tmp = crate::util::gen_temp_file()
                                .with_context(|| {
                                    "Failed to generate temp file for pipeline shell operation"
                                        .to_string()
                                })?
                                .path()
                                .to_path_buf();
                            tmp_files.push(tmp.clone());
                            tmp
                        } else {
                            output_path.clone()
                        }
                    };
                    let command = {
                        command
                            .0
                            .replace("$INPUT", input_path.to_string_lossy().as_ref())
                            .replace("$OUTPUT", artifact_path.to_string_lossy().as_ref())
                    };
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
                    input_path = artifact_path;
                }
            }
        }

        clean_temp_files(&tmp_files)?;

        Ok(())
    }
}

fn clean_temp_files(tmp_files: &[PathBuf]) -> Result<(), anyhow::Error> {
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
    fn multiple_ops() {
        let mut pipeline = Pipeline::new("*.txt").unwrap();
        pipeline.push_op(Operation::Shell(ShellCommand::new(
            r#"sed 's/old/new/g' $INPUT > $OUTPUT"#,
        )));
        pipeline.push_op(Operation::Copy);

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
        assert_eq!(&target_content, "new");
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
    fn shell_command_has_input() {
        let cmd = ShellCommand::new("echo $INPUT");
        assert!(cmd.has_input());
    }

    #[test]
    fn shell_command_has_output() {
        let cmd = ShellCommand::new("echo $OUTPUT");
        assert!(cmd.has_output());
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
