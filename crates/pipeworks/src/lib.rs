use color_eyre::{Section, SectionExt};
use eyre::{eyre, WrapErr};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::str::FromStr;
use tracing::{debug, trace, trace_span};
use typed_path::{AbsPath, RelPath};
use typed_uri::AssetUri;

pub const TMP_ARTIFACT_PREFIX: &str = "pipeworks_artifact_";
pub const OP_COPY: &str = "_COPY_";

pub type Result<T> = eyre::Result<T>;

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

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "_COPY_" => Ok(Self::Copy),
            other => Ok(Self::Shell(ShellCommand(other.to_owned()))),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BaseDir {
    RelativeToDoc(RelPath),
    RelativeToRoot(AbsPath),
}

impl BaseDir {
    pub fn new<P: AsRef<Path>>(base: P) -> Self {
        let base = base.as_ref();
        if let Ok(base) = AbsPath::new(base) {
            return Self::RelativeToRoot(base);
        }
        if let Ok(base) = RelPath::new(base) {
            return Self::RelativeToDoc(base);
        }

        unreachable!(
            "base dir should always be constructable. this is a bug. base: '{base}'",
            base = base.display()
        );
    }

    #[must_use]
    pub fn join(&self, target: &RelPath) -> BaseDir {
        match self {
            Self::RelativeToDoc(rel) => BaseDir::new(rel.join(target)),
            Self::RelativeToRoot(abs) => BaseDir::new(abs.join(target)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Paths {
    root: AbsPath,
    output_dir: RelPath,
    content_dir: RelPath,
}

impl Paths {
    pub fn new(root: &AbsPath, output_dir: &RelPath, content_dir: &RelPath) -> Self {
        Self {
            root: root.clone(),
            output_dir: output_dir.clone(),
            content_dir: content_dir.clone(),
        }
    }

    pub fn root(&self) -> &AbsPath {
        &self.root
    }

    pub fn output_dir(&self) -> &RelPath {
        &self.output_dir
    }

    pub fn content_dir(&self) -> &RelPath {
        &self.content_dir
    }
}

#[derive(Debug, Clone)]
pub struct Pipeline {
    ops: Vec<Operation>,
    base_dir: BaseDir,
    paths: Paths,
}

impl Pipeline {
    pub fn new(paths: Paths, base_dir: &BaseDir) -> Result<Self> {
        Ok(Self {
            ops: vec![],
            base_dir: base_dir.clone(),
            paths,
        })
    }

    pub fn with_ops(paths: Paths, base_dir: &BaseDir, ops: &[Operation]) -> Result<Self> {
        Ok(Self {
            ops: ops.into(),
            base_dir: base_dir.clone(),
            paths,
        })
    }

    pub fn push_op(&mut self, op: Operation) {
        self.ops.push(op);
    }

    pub fn run(&self, asset_uri: &AssetUri) -> Result<()> {
        let mut scratch_files = vec![];
        let result = self.do_run(&mut scratch_files, asset_uri);

        clean_temp_files(&scratch_files).wrap_err("failed to cleanup pipeline scratch files")?;

        result
    }

    fn do_run(&self, scratch_files: &mut Vec<PathBuf>, asset_uri: &AssetUri) -> Result<()> {
        let mut scratch_path = new_scratch_file(scratch_files, &[])
            .wrap_err("Failed to created new scratch file for pipeline processing")?;

        let working_dir: AbsPath = match &self.base_dir {
            BaseDir::RelativeToRoot(base) => {
                let relative_base = base.strip_prefix("/").wrap_err_with(|| {
                    format!(
                        "Failed to strip root prefix(/) from '{}' during pipline processing",
                        base.display()
                    )
                })?;
                let working_dir = self.paths.root().clone().join(&relative_base);

                working_dir
            }
            BaseDir::RelativeToDoc(relative) => {
                let working_dir = asset_uri
                    // get HTML source file
                    .html_src()
                    // convert to sys_path
                    .as_sys_path()
                    // change base to the source base directory
                    .with_base(self.paths.content_dir())
                    // remove file name
                    .pop()
                    // use absolute path so we can change directory
                    .to_absolute_path()
                    // append the relative directory
                    .join(relative);

                working_dir
            }
        };

        let target_path = asset_uri
            .to_target_sys_path(self.paths.root(), self.paths.output_dir())
            .wrap_err("Failed to convert asset uri to SysPath for pipeline processing")?
            .to_absolute_path();

        let src_path = {
            if asset_uri.uri_fragment().starts_with('/') {
                working_dir.join(&RelPath::from_relative(
                    asset_uri.uri_fragment().rsplit_once('/').unwrap().1,
                ))
            } else {
                working_dir.join(&RelPath::from_relative(asset_uri.uri_fragment()))
            }
        };

        // create all parent directories for target file
        std::fs::create_dir_all(&target_path.pop()).wrap_err_with(|| {
            format!(
                "Failed to make parent directories for asset target '{}'",
                &target_path.pop()
            )
        })?;

        // autocopy is enabled whenever we have a shell command that
        // does _not_ use the $TARGET token
        let mut autocopy = false;

        for op in &self.ops {
            match op {
                Operation::Copy => {
                    trace!(
                        operation = "OP_COPY",
                        "copy: {:?} -> {:?}",
                        src_path,
                        target_path
                    );
                    debug!(target: "pylon_user", "copy: {:?} -> {:?}", src_path, target_path);
                    std::fs::copy(&src_path, &target_path).wrap_err_with(||format!("Failed to copy '{src_path}' -> '{target_path}' during pipeline processing"))?;
                }
                Operation::Shell(command) => {
                    if command.0.contains("$TARGET") {
                        autocopy = false;
                    } else {
                        autocopy = true;
                    }

                    let command = {
                        command
                            .0
                            .replace("$SOURCE", &src_path.to_string())
                            .replace("$SCRATCH", scratch_path.to_string_lossy().as_ref())
                            .replace("$TARGET", target_path.to_string().as_str())
                    };

                    if command.contains("$NEW_SCRATCH") {
                        scratch_path =
                            new_scratch_file(scratch_files, &std::fs::read(&scratch_path).wrap_err("Failed to read scratch file during pipeline processing")?)
                                .wrap_err(
                                    "Failed to create new scratch file for shell operation during pipeline processing",
                                )?;
                    }

                    let command =
                        command.replace("$NEW_SCRATCH", scratch_path.to_string_lossy().as_ref());

                    // Output is ignored in pipeline processing and should always be captured
                    // using a command token.
                    let _command_output = run_command(command, &working_dir)?;
                }
            }
        }

        if autocopy {
            std::fs::copy(&scratch_path, &target_path).wrap_err_with(||format!("Failed performing copy operation in pipeline. '{scratch_path:?}' -> '{target_path:?}'"))?;
        }

        Ok(())
    }
}

pub fn run_command<S: AsRef<str>>(command: S, working_dir: &AbsPath) -> Result<String> {
    let command = command.as_ref();

    let cmd = format!(
        "cd {} && {}",
        working_dir.as_path().to_string_lossy(),
        &command
    );
    trace!(command=%cmd, "execute shell command");
    debug!(target: "pylon_user", command=%cmd, "execute shell command");

    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .wrap_err_with(|| format!("Failed running shell command: '{command}'"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(eyre!("Shell command failed to run"))
            .with_section(move || command.to_owned().header("Command:"))
            .with_section(move || stdout.trim().to_string().header("Stdout:"))
            .with_section(move || stderr.trim().to_string().header("Stderr:"))
    }
}

fn new_scratch_file(files: &mut Vec<PathBuf>, content: &[u8]) -> Result<PathBuf> {
    let tmp = tempfile::Builder::new()
        .prefix(TMP_ARTIFACT_PREFIX)
        .rand_bytes(12)
        .tempfile()
        .with_context(|| "failed creating temporary file for shell processing".to_string())
        .wrap_err("Failed to generate temp file for pipeline shell operation")?
        .path()
        .to_path_buf();
    files.push(tmp.clone());
    std::fs::write(&tmp, content).wrap_err("Failed to write contents into scratch file")?;
    Ok(files[files.len() - 1].clone())
}

fn clean_temp_files(tmp_files: &[PathBuf]) -> Result<()> {
    let _span = trace_span!("clean up temp files").entered();
    trace!(files = ?tmp_files);
    for f in tmp_files {
        std::fs::remove_file(&f).wrap_err_with(|| {
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

    #![allow(warnings, unused)]

    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use temptree::temptree;
    use typed_path::{AbsPath, ConfirmedPath, RelPath, SysPath};
    use typed_uri::Uri;

    fn confirmed_html_path(tree: &TempDir, path: &str) -> ConfirmedPath<pathmarker::HtmlFile> {
        let path = SysPath::from_abs_path(
            &AbsPath::new(tree.path().join(path)).unwrap(),
            &AbsPath::new(tree.path()).unwrap(),
            &RelPath::new("target").unwrap(),
        )
        .expect("failed to make syspath for html file");
        path.confirm(pathmarker::HtmlFile)
            .expect("failed to make confirmed path")
    }

    fn make_paths(tree: &TempDir) -> Paths {
        Paths {
            output_dir: RelPath::from_relative("target"),
            root: AbsPath::from_absolute(tree.path()),
            content_dir: RelPath::from_relative("src"),
        }
    }

    #[test]
    fn new_with_ops() {
        let tree = temptree! {
            "rules.rhai": "",
            templates: {},
            target: {},
            src: {},
            syntax_themes: {},
        };
        let paths = make_paths(&tree);

        let ops = vec![Operation::Copy];

        let pipeline = Pipeline::with_ops(paths, &BaseDir::new("/"), ops.as_slice());
        assert!(pipeline.is_ok());
    }

    #[test]
    fn op_copy_with_root_base_single_dir() {
        let tree = temptree! {
            "rules.rhai": "",
            templates: {},
            target: {
                "output.html": "",
            },
            src: {
                "test.txt": "data",
            },
            syntax_themes: {},
        };

        let paths = make_paths(&tree);

        let mut pipeline = Pipeline::new(paths, &BaseDir::new("/src")).unwrap();

        pipeline.push_op(Operation::Copy);

        let html_file = confirmed_html_path(&tree, "target/output.html");
        let asset_uri = Uri::new("/test.txt", "/test.txt")
            .unwrap()
            .to_asset_uri(&html_file);

        pipeline.run(&asset_uri).expect("failed to run pipeline");

        let target_content = fs::read_to_string(tree.path().join("target/test.txt")).unwrap();
        assert_eq!(&target_content, "data");
    }

    #[test]
    fn op_copy_with_root_base() {
        let tree = temptree! {
            "rules.rhai": "",
            templates: {},
            target: {
                "output.html": "",
            },
            src: {},
            "test.txt": "data",
            syntax_themes: {},
        };

        let paths = make_paths(&tree);

        let mut pipeline = Pipeline::new(paths, &BaseDir::new("/")).unwrap();

        pipeline.push_op(Operation::Copy);

        let html_file = confirmed_html_path(&tree, "target/output.html");
        let asset_uri = Uri::new("/test.txt", "/test.txt")
            .unwrap()
            .to_asset_uri(&html_file);

        pipeline.run(&asset_uri).expect("failed to run pipeline");

        let target_content = fs::read_to_string(tree.path().join("target/test.txt")).unwrap();
        assert_eq!(&target_content, "data");
    }

    #[test]
    fn op_copy_with_root_base_fails_when_missing_src_file() {
        let tree = temptree! {
            "rules.rhai": "",
            templates: {},
            target: {
                "output.html": "",
            },
            src: {},
            syntax_themes: {},
        };

        let paths = make_paths(&tree);

        let mut pipeline = Pipeline::new(paths, &BaseDir::new("/")).unwrap();

        pipeline.push_op(Operation::Copy);

        let html_file = confirmed_html_path(&tree, "target/output.html");
        let asset_uri = Uri::new("/test.txt", "/test.txt")
            .unwrap()
            .to_asset_uri(&html_file);

        let status = pipeline.run(&asset_uri);
        assert!(status.is_err());
    }

    #[test]
    fn op_copy_with_root_base_single_dir_fails_when_missing_src_file() {
        let tree = temptree! {
            "rules.rhai": "",
            templates: {},
            target: {
                "output.html": "",
            },
            src: {},
            syntax_themes: {},
        };

        let paths = make_paths(&tree);

        let mut pipeline = Pipeline::new(paths, &BaseDir::new("/src")).unwrap();

        pipeline.push_op(Operation::Copy);

        let html_file = confirmed_html_path(&tree, "target/output.html");
        let asset_uri = Uri::new("/test.txt", "/test.txt")
            .unwrap()
            .to_asset_uri(&html_file);

        let status = pipeline.run(&asset_uri);
        assert!(status.is_err());
    }

    #[test]
    fn op_copy_with_relative_base() {
        let tree = temptree! {
            "rules.rhai": "",
            templates: {},
            target: {
                "output.html": "",
            },
            src: {
                "test.txt": "data",
            },
            syntax_themes: {},
        };

        let paths = make_paths(&tree);

        let mut pipeline = Pipeline::new(paths, &BaseDir::new(".")).unwrap();

        pipeline.push_op(Operation::Copy);

        let html_file = confirmed_html_path(&tree, "target/output.html");
        let asset_uri = Uri::new("/test.txt", "/test.txt")
            .unwrap()
            .to_asset_uri(&html_file);

        pipeline.run(&asset_uri).expect("failed to run pipeline");

        let target_content = fs::read_to_string(tree.path().join("target/test.txt")).unwrap();
        assert_eq!(&target_content, "data");
    }

    #[test]
    fn op_copy_with_relative_base_in_subdir() {
        let tree = temptree! {
            "rules.rhai": "",
            templates: {},
            target: {
                inner: {
                    "output.html": "",
                }
            },
            src: {
                inner: {
                    "test.txt": "data",
                }
            },
            syntax_themes: {},
        };

        let paths = make_paths(&tree);

        let mut pipeline = Pipeline::new(paths, &BaseDir::new(".")).unwrap();

        pipeline.push_op(Operation::Copy);

        let html_file = confirmed_html_path(&tree, "target/inner/output.html");
        let asset_uri = Uri::new("/inner/test.txt", "/inner/test.txt")
            .unwrap()
            .to_asset_uri(&html_file);

        pipeline.run(&asset_uri).expect("failed to run pipeline");

        let target_content = fs::read_to_string(tree.path().join("target/inner/test.txt")).unwrap();
        assert_eq!(&target_content, "data");
    }

    #[test]
    fn op_copy_with_relative_base_access_subdir() {
        let tree = temptree! {
            "rules.rhai": "",
            templates: {},
            target: {
                inner: {
                    "output.html": "",
                }
            },
            src: {
                inner: {
                    colocated: {
                        "test.txt": "data",
                    }
                }
            },
            syntax_themes: {},
        };

        let paths = make_paths(&tree);

        let mut pipeline = Pipeline::new(paths, &BaseDir::new("./colocated")).unwrap();

        pipeline.push_op(Operation::Copy);

        let html_file = confirmed_html_path(&tree, "target/inner/output.html");
        let asset_uri = Uri::new("/inner/test.txt", "/inner/test.txt")
            .unwrap()
            .to_asset_uri(&html_file);

        pipeline.run(&asset_uri).expect("failed to run pipeline");

        let target_content = fs::read_to_string(tree.path().join("target/inner/test.txt")).unwrap();
        assert_eq!(&target_content, "data");
    }

    #[test]
    fn op_shell_direct_target_write() {
        let tree = temptree! {
            "rules.rhai": "",
            templates: {},
            target: {
                "output.html": "",
            },
            src: {},
            "test.txt": "old",
            syntax_themes: {},
        };

        let paths = make_paths(&tree);

        let mut pipeline = Pipeline::new(paths, &BaseDir::new("/")).unwrap();

        pipeline.push_op(Operation::Shell(ShellCommand::new(
            "sed 's/old/new/g' $SOURCE > $TARGET",
        )));

        let html_file = confirmed_html_path(&tree, "target/output.html");
        let asset_uri = Uri::new("/test.txt", "/test.txt")
            .unwrap()
            .to_asset_uri(&html_file);

        pipeline.run(&asset_uri).expect("failed to run pipeline");

        let target_content = fs::read_to_string(tree.path().join("target/test.txt")).unwrap();
        assert_eq!(&target_content, "new");
    }

    #[test]
    fn op_shell_changes_working_dir_when_absolute() {
        let tree = temptree! {
            "rules.rhai": "",
            templates: {},
            target: {
                "output.html": "",
            },
            src: {},
            random: {
                a: {
                    b: {
                        "test.txt": "old",
                    }
                }
            },
            syntax_themes: {},
        };

        let paths = make_paths(&tree);

        let mut pipeline = Pipeline::new(paths, &BaseDir::new("/random/a/b")).unwrap();

        pipeline.push_op(Operation::Shell(ShellCommand::new(
            "sed 's/old/new/g' test.txt > $TARGET",
        )));

        let html_file = confirmed_html_path(&tree, "target/output.html");
        let asset_uri = Uri::new("/test.txt", "/test.txt")
            .unwrap()
            .to_asset_uri(&html_file);

        pipeline.run(&asset_uri).expect("failed to run pipeline");

        let target_content = fs::read_to_string(tree.path().join("target/test.txt")).unwrap();
        assert_eq!(&target_content, "new");
    }

    #[test]
    fn op_shell_changes_working_dir_when_relative() {
        let tree = temptree! {
            "rules.rhai": "",
            templates: {},
            target: {
                a: {
                    b: {
                        "output.html": "",
                    }
                }
            },
            src: {
                a: {
                    b: {
                        data: {
                            "test.txt": "old"
                        }
                    }
                }
            },
            syntax_themes: {},
        };

        let paths = make_paths(&tree);

        let mut pipeline = Pipeline::new(paths, &BaseDir::new("./data")).unwrap();

        pipeline.push_op(Operation::Shell(ShellCommand::new(
            "sed 's/old/new/g' test.txt > $TARGET",
        )));

        let html_file = confirmed_html_path(&tree, "target/a/b/output.html");
        let asset_uri = Uri::new("/a/b/test.txt", "/a/b/test.txt")
            .unwrap()
            .to_asset_uri(&html_file);

        pipeline.run(&asset_uri).expect("failed to run pipeline");

        let target_content = fs::read_to_string(tree.path().join("target/a/b/test.txt")).unwrap();
        assert_eq!(&target_content, "new");
    }

    #[test]
    fn op_shell_new_scratch_autocopy() {
        let tree = temptree! {
            "rules.rhai": "",
            templates: {},
            target: {
                "output.html": "",
            },
            src: {},
            "test.txt": "old",
            syntax_themes: {},
        };

        let paths = make_paths(&tree);

        let mut pipeline = Pipeline::new(paths, &BaseDir::new("/")).unwrap();

        pipeline.push_op(Operation::Shell(ShellCommand::new(
            "sed 's/old/new/g' $SOURCE > $NEW_SCRATCH",
        )));

        let html_file = confirmed_html_path(&tree, "target/output.html");
        let asset_uri = Uri::new("/test.txt", "/test.txt")
            .unwrap()
            .to_asset_uri(&html_file);

        pipeline.run(&asset_uri).expect("failed to run pipeline");

        let target_content = fs::read_to_string(tree.path().join("target/test.txt")).unwrap();
        assert_eq!(&target_content, "new");
    }

    #[test]
    fn op_shell_new_scratch_no_autocopy() {
        let tree = temptree! {
            "rules.rhai": "",
            templates: {},
            target: {
                "output.html": "",
            },
            src: {},
            "test.txt": "old",
            syntax_themes: {},
        };

        let paths = make_paths(&tree);

        let mut pipeline = Pipeline::new(paths, &BaseDir::new("/")).unwrap();

        pipeline.push_op(Operation::Shell(ShellCommand::new(
            "sed 's/old/new/g' $SOURCE > $NEW_SCRATCH",
        )));

        pipeline.push_op(Operation::Shell(ShellCommand::new("cp $SCRATCH $TARGET")));

        let html_file = confirmed_html_path(&tree, "target/output.html");
        let asset_uri = Uri::new("/test.txt", "/test.txt")
            .unwrap()
            .to_asset_uri(&html_file);

        pipeline.run(&asset_uri).expect("failed to run pipeline");

        let target_content = fs::read_to_string(tree.path().join("target/test.txt")).unwrap();
        assert_eq!(&target_content, "new");
    }

    #[test]
    fn op_shell_new_scratch_no_autocopy_relative() {
        let tree = temptree! {
            "rules.rhai": "",
            templates: {},
            target: {
                inner: {
                    "output.html": "",
                }
            },
            src: {
                inner: {
                    "test.txt": "old",
                }
            },
            syntax_themes: {},
        };

        let paths = make_paths(&tree);

        let mut pipeline = Pipeline::new(paths, &BaseDir::new(".")).unwrap();

        pipeline.push_op(Operation::Shell(ShellCommand::new(
            "sed 's/old/new/g' $SOURCE > $NEW_SCRATCH",
        )));

        pipeline.push_op(Operation::Shell(ShellCommand::new("cp $SCRATCH $TARGET")));

        let html_file = confirmed_html_path(&tree, "target/inner/output.html");
        let asset_uri = Uri::new("/inner/test.txt", "/inner/test.txt")
            .unwrap()
            .to_asset_uri(&html_file);

        pipeline.run(&asset_uri).expect("failed to run pipeline");

        let target_content = fs::read_to_string(tree.path().join("target/inner/test.txt")).unwrap();
        assert_eq!(&target_content, "new");
    }

    #[test]
    fn op_shell_new_scratch_no_autocopy_relative_nested() {
        let tree = temptree! {
            "rules.rhai": "",
            templates: {},
            target: {
                inner: {
                    "output.html": "",
                }
            },
            src: {
                inner: {
                    asset: {
                        "test.txt": "old",
                    }
                }
            },
            syntax_themes: {},
        };

        let paths = make_paths(&tree);

        let mut pipeline = Pipeline::new(paths, &BaseDir::new("./asset")).unwrap();

        pipeline.push_op(Operation::Shell(ShellCommand::new(
            "sed 's/old/new/g' test.txt > $NEW_SCRATCH",
        )));

        pipeline.push_op(Operation::Shell(ShellCommand::new("cp $SCRATCH $TARGET")));

        let html_file = confirmed_html_path(&tree, "target/inner/output.html");
        let asset_uri = Uri::new("/inner/test.txt", "/inner/test.txt")
            .unwrap()
            .to_asset_uri(&html_file);

        pipeline.run(&asset_uri).expect("failed to run pipeline");

        let target_content = fs::read_to_string(tree.path().join("target/inner/test.txt")).unwrap();
        assert_eq!(&target_content, "new");
    }

    #[test]
    fn op_shell_source_token_works_with_relative_path() {
        let tree = temptree! {
            "rules.rhai": "",
            templates: {},
            target: {
                inner: {
                    "output.html": "",
                }
            },
            src: {
                inner: {
                    asset: {
                        "test.txt": "old",
                    }
                }
            },
            syntax_themes: {},
        };

        let paths = make_paths(&tree);

        let mut pipeline = Pipeline::new(paths, &BaseDir::new("./asset")).unwrap();

        pipeline.push_op(Operation::Shell(ShellCommand::new(
            "sed 's/old/new/g' $SOURCE > $NEW_SCRATCH",
        )));

        pipeline.push_op(Operation::Shell(ShellCommand::new("cp $SCRATCH $TARGET")));

        let html_file = confirmed_html_path(&tree, "target/inner/output.html");
        let asset_uri = Uri::new("/inner/test.txt", "/inner/test.txt")
            .unwrap()
            .to_asset_uri(&html_file);

        pipeline.run(&asset_uri).expect("failed to run pipeline");

        let target_content = fs::read_to_string(tree.path().join("target/inner/test.txt")).unwrap();
        assert_eq!(&target_content, "new");
    }

    #[test]
    fn op_shell_direct_target_write_makes_needed_subdirs() {
        let tree = temptree! {
            "rules.rhai": "",
            templates: {},
            target: {
                "output.html": "",
            },
            src: {},
            "test.txt": "old",
            syntax_themes: {},
        };

        let paths = make_paths(&tree);

        let mut pipeline = Pipeline::new(paths, &BaseDir::new("/")).unwrap();

        pipeline.push_op(Operation::Shell(ShellCommand::new("echo test > $TARGET")));

        let html_file = confirmed_html_path(&tree, "target/output.html");
        let asset_uri = Uri::new("/static/styles/site.css", "/static/styles/site.css")
            .unwrap()
            .to_asset_uri(&html_file);

        pipeline.run(&asset_uri).expect("failed to run pipeline");

        let target_content =
            fs::read_to_string(tree.path().join("target/static/styles/site.css")).unwrap();
        assert_eq!(&target_content, "test\n");
    }

    #[test]
    fn operation_fromstr_impl_copy() {
        use std::str::FromStr;

        let operation = Operation::from_str("_COPY_").unwrap();
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
    fn fails_on_broken_shell_op() {
        let tree = temptree! {
            "rules.rhai": "",
            templates: {},
            target: {
                "output.html": "",
            },
            src: {},
            "test.txt": "old",
            syntax_themes: {},
        };

        let paths = make_paths(&tree);

        let mut pipeline = Pipeline::new(paths, &BaseDir::new("/")).unwrap();

        pipeline.push_op(Operation::Shell(ShellCommand::new("CMD_NOT_FOUND")));

        let html_file = confirmed_html_path(&tree, "target/output.html");
        let asset_uri = Uri::new("/test.txt", "/test.txt")
            .unwrap()
            .to_asset_uri(&html_file);

        let result = pipeline.run(&asset_uri);
        assert!(result.is_err());
    }

    #[test]
    fn basedir_joins_when_relative_to_doc() {
        let basedir = BaseDir::new("a");

        let joined = basedir.join(&RelPath::from_relative("b"));
        assert_eq!(joined, BaseDir::new("a/b"));
    }

    #[test]
    fn basedir_joins_when_relative_to_root() {
        let basedir = BaseDir::new("/a");

        let joined = basedir.join(&RelPath::from_relative("b"));
        assert_eq!(joined, BaseDir::new("/a/b"));
    }
}
