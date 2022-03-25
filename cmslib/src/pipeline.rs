use anyhow::Context;
use std::path::{Path, PathBuf};
use tracing::info;

#[derive(Debug)]
pub struct GlobCandidate<'a>(globset::Candidate<'a>);

impl<'a> GlobCandidate<'a> {
    pub fn new<P: AsRef<Path>>(path: &'a P) -> GlobCandidate<'a> {
        Self(globset::Candidate::new(path))
    }
}

#[derive(Debug, Clone)]
pub struct Glob {
    glob: globset::Glob,
    matcher: globset::GlobMatcher,
}

impl Glob {
    pub fn is_match<P: AsRef<Path>>(&self, path: P) -> bool {
        self.matcher.is_match(path)
    }
    pub fn is_match_candidate(&self, path: &GlobCandidate<'_>) -> bool {
        self.matcher.is_match_candidate(&path.0)
    }
}

impl TryFrom<String> for Glob {
    type Error = globset::Error;
    fn try_from(s: String) -> Result<Glob, Self::Error> {
        let glob = globset::GlobBuilder::new(&s)
            .literal_separator(true)
            .build()?;
        let matcher = glob.compile_matcher();
        Ok(Self { glob, matcher })
    }
}

impl TryFrom<&str> for Glob {
    type Error = globset::Error;
    fn try_from(s: &str) -> Result<Glob, Self::Error> {
        let glob = globset::GlobBuilder::new(s)
            .literal_separator(true)
            .build()?;
        let matcher = glob.compile_matcher();
        Ok(Self { glob, matcher })
    }
}

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

#[derive(Clone, Debug)]
pub enum AutorunTrigger {
    CustomGlob(Glob),
    TargetGlob,
}

#[derive(Debug)]
pub struct Pipeline {
    pub target_glob: Glob,
    ops: Vec<Operation>,
    autorun: AutorunTrigger,
}

impl Pipeline {
    pub fn new<G: TryInto<Glob, Error = globset::Error>>(
        target_glob: G,
        autorun: AutorunTrigger,
    ) -> Result<Self, anyhow::Error> {
        let target_glob = target_glob.try_into()?;

        Ok(Self {
            target_glob,
            autorun,
            ops: vec![],
        })
    }

    pub fn push_op(&mut self, op: Operation) {
        self.ops.push(op);
    }

    pub fn is_match<P: AsRef<Path>>(&self, asset: P) -> bool {
        self.target_glob.is_match(asset)
    }

    pub fn is_match_candidate<'a, C: AsRef<GlobCandidate<'a>>>(&self, asset: C) -> bool {
        self.target_glob.is_match_candidate(asset.as_ref())
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
        for op in self.ops.iter() {
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
