#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::enum_glob_use)]
#![allow(clippy::implicit_hasher)]
#![allow(clippy::match_bool)]
#![allow(clippy::match_same_arms)]
// TODO: delete these after writing docs
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::missing_errors_doc)]

pub mod core;
pub mod devserver;
pub mod discover;
pub mod init;
pub mod postprocess;
pub mod render;
pub mod site_context;
pub mod util;

pub use render::Renderers;
pub use typed_path::*;

use thiserror::Error;

pub type Result<T> = eyre::Result<T>;

pub const USER_LOG: &str = "pylon_user";

#[derive(Error, Debug)]
#[error(transparent)]
pub struct AsStdError(#[from] eyre::Report);

#[cfg(test)]
pub(crate) mod test {

    use std::sync::Arc;
    use tempfile::TempDir;
    use temptree::temptree;
    use typed_path::{ConfirmedPath, SysPath};

    use crate::{
        core::engine::{EnginePaths, GlobalEnginePaths},
        AbsPath, RelPath,
    };

    macro_rules! abs {
        ($path:literal) => {{
            &crate::AbsPath::new($path).unwrap()
        }};
        ($path:expr) => {{
            &crate::AbsPath::new($path).unwrap()
        }};
    }

    macro_rules! rel {
        ($path:literal) => {{
            &crate::RelPath::new($path).unwrap()
        }};
        ($path:expr) => {{
            &crate::RelPath::new($path).unwrap()
        }};
    }

    pub(crate) use abs;
    pub(crate) use rel;

    pub fn default_test_paths(tree: &TempDir) -> GlobalEnginePaths {
        Arc::new(EnginePaths {
            rule_script: RelPath::new("rules.rhai").unwrap(),
            content_dir: RelPath::new("src").unwrap(),
            syntax_theme_dir: RelPath::new("syntax_themes").unwrap(),
            output_dir: RelPath::new("target").unwrap(),
            template_dir: RelPath::new("templates").unwrap(),
            project_root: AbsPath::new(tree.path()).unwrap(),
        })
    }

    pub fn simple_init() -> (GlobalEnginePaths, TempDir) {
        let tree = temptree! {
          "rules.rhai": "",
          templates: {
              "default.tera": "",
          },
          target: {},
          src: {},
          syntax_themes: {}
        };
        let paths = default_test_paths(&tree);

        (paths, tree)
    }

    pub fn checked_md_path(tree: &TempDir, path: &str) -> ConfirmedPath<pathmarker::MdFile> {
        let path = SysPath::from_abs_path(
            &AbsPath::new(tree.path().join(path)).unwrap(),
            &AbsPath::new(tree.path()).unwrap(),
            &RelPath::new("src").unwrap(),
        )
        .expect("failed to make syspath for md file");
        path.confirm(pathmarker::MdFile)
            .expect("failed to make confirmed path")
    }
}
