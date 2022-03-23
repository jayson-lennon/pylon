use anyhow::Context;
use regex::Regex;
use std::path::{Path, PathBuf};
use tracing::info;

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
pub struct Glob(pub String);
impl AsRef<str> for Glob {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl From<String> for Glob {
    fn from(s: String) -> Self {
        Glob(s)
    }
}

impl From<&str> for Glob {
    fn from(s: &str) -> Self {
        Glob(s.to_owned())
    }
}

impl From<Glob> for String {
    fn from(g: Glob) -> Self {
        g.0
    }
}

#[derive(Clone, Debug)]
pub enum Operation {
    Copy,
    Shell(ShellCommand),
}

#[derive(Clone, Debug)]
pub enum AutorunTrigger {
    CustomGlob(Glob),
    TargetGlob,
}

#[derive(Debug)]
struct PipelineState {
    re: Regex,
}

#[derive(Debug)]
pub struct PipelineConfig {
    pub target_glob: Glob,
    ops: Vec<Operation>,
    autorun: AutorunTrigger,
}

#[derive(Debug)]
pub struct Pipeline {
    pub config: PipelineConfig,
    state: PipelineState,
}

impl Pipeline {
    pub fn new<G: Into<Glob>>(
        target_glob: G,
        autorun: AutorunTrigger,
    ) -> Result<Self, anyhow::Error> {
        let target_glob = target_glob.into();
        let re = crate::util::glob_to_re(&target_glob).with_context(|| {
            format!("Failed converting glob to regex when creating new pipeline")
        })?;
        let config = PipelineConfig {
            target_glob,
            autorun,
            ops: vec![],
        };
        Ok(Self {
            config,
            state: PipelineState { re },
        })
    }

    pub fn push_op(&mut self, op: Operation) {
        self.config.ops.push(op);
    }

    pub fn is_match<P: AsRef<str>>(&self, asset: P) -> bool {
        self.state.re.is_match(asset.as_ref())
    }

    pub fn run<P: AsRef<Path>>(
        &self,
        src_root: P,
        output_root: P,
        target_asset: P,
    ) -> Result<(), anyhow::Error> {
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
        // let mut input_path = self.dirs.abs_src_asset(target_asset);
        // let output_path = self.dirs.abs_target_asset(target_asset);
        for op in self.config.ops.iter() {
            let _span = tracing::info_span!(target: "pipeline_spans", "perform pipeline operation")
                .entered();
            match op {
                Operation::Copy => {
                    info!(target: "pipeline_event", "copy: {:?} -> {:?}", input_path, output_path);
                    std::fs::copy(&input_path, &output_path).with_context(||format!("Failed performing copy operation in pipeline. '{input_path:?}' -> '{output_path:?}'"))?;
                }
                Operation::Shell(command) => {
                    info!(target: "pipeline_event", "shell command: {:?}", command);
                    let artifact_path = {
                        if command.has_output() {
                            let tmp = crate::util::gen_temp_file()
                                .with_context(|| {
                                    format!(
                                        "Failed to generate temp file for pipeline shell operation"
                                    )
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
                        let mut command = std::process::Command::new("sh")
                            .arg("-c")
                            .arg(&command)
                            .spawn()
                            .with_context(|| {
                                format!("Failed running shell pipeline command: '{command}'")
                            })?;
                        command.wait().with_context(|| {
                            format!("Failed waiting for child process in shell pipeline processing")
                        })?;
                    }
                    input_path = artifact_path;
                }
            }
        }
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
}