use crate::Directories;
use regex::Regex;
use std::path::Path;

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
struct PipelineConfig {
    target_glob: Glob,
    ops: Vec<Operation>,
    autorun: AutorunTrigger,
}

#[derive(Debug)]
pub struct Pipeline {
    config: PipelineConfig,
    dirs: Directories,
    state: PipelineState,
}

impl Pipeline {
    pub fn new(dirs: Directories, target_glob: Glob, autorun: AutorunTrigger) -> Self {
        let re = crate::glob_to_re(target_glob.clone());
        let config = PipelineConfig {
            target_glob,
            autorun,
            ops: vec![],
        };
        Self {
            config,
            dirs,
            state: PipelineState { re },
        }
    }

    pub fn push_op(&mut self, op: Operation) {
        self.config.ops.push(op);
    }

    pub fn is_match<P: AsRef<str>>(&self, asset: P) -> bool {
        self.state.re.is_match(asset.as_ref())
    }

    pub fn run<P: AsRef<Path>>(&mut self, asset: P) {
        let asset = asset.as_ref();
        let mut input_path = self.dirs.abs_src_asset(asset);
        let output_path = self.dirs.abs_target_asset(asset);
        for op in self.config.ops.iter() {
            match op {
                Operation::Copy => {
                    println!("Copy {input_path:?} -> {output_path:?}");
                    let _ = std::fs::copy(&input_path, &output_path).expect("failed to copy");
                }
                Operation::Shell(command) => {
                    let artifact_path = {
                        if command.has_output() {
                            crate::gen_temp_file().path().to_path_buf()
                        } else {
                            output_path.clone()
                        }
                    };
                    let command = {
                        command
                            .0
                            .replace(
                                "$INPUT",
                                input_path.to_str().expect("non UTF-8 path encountered"),
                            )
                            .replace(
                                "$OUTPUT",
                                artifact_path.to_str().expect("non UTF-8 path encountered"),
                            )
                    };
                    println!("run cmd: {:?}", command);
                    {
                        let mut command = std::process::Command::new("sh")
                            .arg("-c")
                            .arg(command)
                            .spawn()
                            .expect("whoops");
                        command.wait().expect("msdouble whoopsg");
                    }
                    input_path = artifact_path;
                }
            }
        }
    }
}
