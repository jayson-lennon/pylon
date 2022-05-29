use color_eyre::{Section, SectionExt};
use eyre::{eyre, WrapErr};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::str::FromStr;
use tracing::{info_span, trace, trace_span};
use typed_path::{AbsPath, RelPath};
use typed_uri::AssetUri;

pub const TMP_ARTIFACT_PREFIX: &str = "pipeworks_artifact_";

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
            "[COPY]" => Ok(Self::Copy),
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
    src_dir: RelPath,
}

impl Paths {
    pub fn new(root: &AbsPath, output_dir: &RelPath, src_dir: &RelPath) -> Self {
        Self {
            root: root.clone(),
            output_dir: output_dir.clone(),
            src_dir: src_dir.clone(),
        }
    }

    pub fn root(&self) -> &AbsPath {
        &self.root
    }

    pub fn output_dir(&self) -> &RelPath {
        &self.output_dir
    }

    pub fn src_dir(&self) -> &RelPath {
        &self.src_dir
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

    #[allow(clippy::too_many_lines)]
    fn do_run(&self, scratch_files: &mut Vec<PathBuf>, asset_uri: &AssetUri) -> Result<()> {
        let mut scratch_path = new_scratch_file(scratch_files, &[])
            .wrap_err("Failed to created new scratch file for pipeline processing")?;

        let mut autocopy = false;

        let target_path = asset_uri
            .to_target_sys_path(self.paths.root(), self.paths.output_dir())
            .wrap_err("Failed to convert asset uri to SysPath for pipeline processing")?;

        for op in &self.ops {
            let _span = info_span!("perform pipeline operation").entered();

            match op {
                Operation::Copy => {
                    let src_path = match &self.base_dir {
                        // Base           URI in HTML page                   filesystem location
                        // ----           -------------------------          ----------------------------
                        // "/"            "/static/styles/site.css"          $ROOT/static/styles/site.css
                        // "/wwwroot"     "/static/styles/site.css"          $ROOT/wwwroot/static/styles/site.css
                        BaseDir::RelativeToRoot(base) => {
                            let base = base.strip_prefix("/").wrap_err_with(||format!("Failed to strip root prefix (/) from '{}' during pipeline processing", base.display()))?;
                            // Change the "base" directory to whatever is supplied by the user.
                            target_path.with_base(&base)
                        }
                        // Base           URI in HTML page                   filesystem location
                        // ----           -------------------------          ----------------------------
                        // ".",           "**/blog/**/diagram.js"            $ROOT/**/blog/**/diagram.js
                        // "./_src",      "**/blog/**/diagram.js"            $ROOT/**/blog/**/_src/diagram.js
                        // ".",           "**/blog/**/*.png"                 $ROOT/**/blog/**/*.png
                        BaseDir::RelativeToDoc(relative) => {
                            target_path
                                // Change the base directory from output to source (where the markdown files are located)
                                .with_base(self.paths.src_dir())
                                // Remove the target file name so we have the parent directory to work with
                                .pop()
                                // Push the supplied relative path onto the existing path
                                .push(relative)
                                // Push the target file name back onto the path (this should always work)
                                .push(&target_path.file_name().try_into().wrap_err(
                                    "Missing file name from target path during pipeline processing",
                                )?)
                        }
                    };
                    trace!("copy: {:?} -> {:?}", src_path, target_path);
                    std::fs::copy(&src_path.to_absolute_path(), &target_path.to_absolute_path()).wrap_err_with(||format!("Failed to copy '{src_path}' -> '{target_path}' during pipeline processing"))?;
                }
                Operation::Shell(command) => {
                    trace!("shell command: {:?}", command);
                    if command.0.contains("$TARGET") {
                        autocopy = false;
                    } else {
                        autocopy = true;
                    }

                    let (working_dir, src_path): (AbsPath, RelPath) = match &self.base_dir {
                        // Base           HTML page                URI in HTML page        working dir              target file name (src path)
                        // ----           ---------------------    -------------------     ---------------------    ---------------
                        // "/"            /blog/entry/post.html    /blog/entry/img.png     $ROOT/                   $ROOT/blog/entry/img.png
                        // "/wwwroot"     /blog/entry/post.html    /blog/entry/img.png     $ROOT/wwwroot/           $ROOT/wwwroot/blog/entry/img.png
                        BaseDir::RelativeToRoot(base) => {
                            let relative_base = base.strip_prefix("/").wrap_err_with(||format!("Failed to strip root prefix(/) from '{}' during pipline processing", base.display()))?;
                            let asset_uri_without_root = &asset_uri.as_str()[1..];

                            let working_dir = self.paths.root().clone();
                            let relative_asset_path =
                                relative_base.join(&asset_uri_without_root.try_into().wrap_err("Failed to create relative path from root base directory during pipeline processing")?);

                            (working_dir, relative_asset_path)
                        }
                        // Base           HTML page                URI in HTML page        working dir                target file name (src path)
                        // ----           ---------------------    -------------------     ---------------------      ---------------
                        // "."            /blog/entry/post.html    /blog/entry/img.png     $ROOT/src/blog/entry/      img.png
                        // "./src"        /blog/entry/post.html    /blog/entry/img.png     $ROOT/src/blog/entry/src/  img.png
                        BaseDir::RelativeToDoc(relative) => {
                            let working_dir = asset_uri
                                // get HTML source file
                                .html_src()
                                // convert to sys_path
                                .as_sys_path()
                                // change base to the source base directory
                                .with_base(self.paths.src_dir())
                                // remove file name
                                .pop()
                                // change to absolute path so we can change directory
                                .to_absolute_path();

                            let asset_name = PathBuf::from(asset_uri.as_str());
                            let asset_name = asset_name
                                .file_name()
                                .ok_or_else(|| eyre!("failed to located filename in asset uri"))?;
                            let relative_asset_path = relative.join(&RelPath::new(asset_name).wrap_err_with(||format!("Failed to create relative path from asset '{}' during pipeline processing", asset_name.to_string_lossy()))?);

                            (working_dir, relative_asset_path)
                        }
                    };

                    let command = {
                        let target = asset_uri
                            .to_target_sys_path(self.paths.root(), self.paths.output_dir())
                            .wrap_err(
                                "Failed to convert asset URI to SysPath during pipeline processing",
                            )?
                            .to_absolute_path();

                        std::fs::create_dir_all(&target.pop()).wrap_err_with(|| {
                            format!(
                                "Failed to make parent directories for asset target '{}'",
                                &target.pop()
                            )
                        })?;

                        command
                            .0
                            .replace("$SOURCE", src_path.to_string().as_str())
                            .replace("$SCRATCH", scratch_path.to_string_lossy().as_ref())
                            .replace("$TARGET", target.to_string().as_str())
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
            std::fs::copy(&scratch_path, &target_path.to_absolute_path()).wrap_err_with(||format!("Failed performing copy operation in pipeline. '{scratch_path:?}' -> '{target_path:?}'"))?;
        }

        Ok(())
    }
}

pub fn run_command<S: AsRef<str>>(command: S, working_dir: &AbsPath) -> Result<String> {
    let command = command.as_ref();
    trace!("command= {:?}", command);
    {
        let cmd = format!(
            "cd {} && {}",
            working_dir.as_path().to_string_lossy(),
            &command
        );
        trace!("cmd= {:?}", cmd);

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
        path.to_confirmed_path(pathmarker::HtmlFile)
            .expect("failed to make checked path")
    }

    fn make_paths(tree: &TempDir) -> Paths {
        Paths {
            output_dir: RelPath::from_relative("target"),
            root: AbsPath::from_absolute(tree.path()),
            src_dir: RelPath::from_relative("src"),
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
        let asset_uri = Uri::new("/test.txt").unwrap().to_based_uri(&html_file);

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
        let asset_uri = Uri::new("/test.txt").unwrap().to_based_uri(&html_file);

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
        let asset_uri = Uri::new("/test.txt").unwrap().to_based_uri(&html_file);

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
        let asset_uri = Uri::new("/test.txt").unwrap().to_based_uri(&html_file);

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
        let asset_uri = Uri::new("/test.txt").unwrap().to_based_uri(&html_file);

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
        let asset_uri = Uri::new("/inner/test.txt")
            .unwrap()
            .to_based_uri(&html_file);

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
        let asset_uri = Uri::new("/inner/test.txt")
            .unwrap()
            .to_based_uri(&html_file);

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
        let asset_uri = Uri::new("/test.txt").unwrap().to_based_uri(&html_file);

        pipeline.run(&asset_uri).expect("failed to run pipeline");

        let target_content = fs::read_to_string(tree.path().join("target/test.txt")).unwrap();
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
        let asset_uri = Uri::new("/test.txt").unwrap().to_based_uri(&html_file);

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
        let asset_uri = Uri::new("/test.txt").unwrap().to_based_uri(&html_file);

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
        let asset_uri = Uri::new("/inner/test.txt")
            .unwrap()
            .to_based_uri(&html_file);

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
            "sed 's/old/new/g' $SOURCE > $NEW_SCRATCH",
        )));

        pipeline.push_op(Operation::Shell(ShellCommand::new("cp $SCRATCH $TARGET")));

        let html_file = confirmed_html_path(&tree, "target/inner/output.html");
        let asset_uri = Uri::new("/inner/test.txt")
            .unwrap()
            .to_based_uri(&html_file);

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
        let asset_uri = Uri::new("/static/styles/site.css")
            .unwrap()
            .to_based_uri(&html_file);

        pipeline.run(&asset_uri).expect("failed to run pipeline");

        let target_content =
            fs::read_to_string(tree.path().join("target/static/styles/site.css")).unwrap();
        assert_eq!(&target_content, "test\n");
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
        let asset_uri = Uri::new("/test.txt").unwrap().to_based_uri(&html_file);

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
