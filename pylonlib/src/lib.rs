#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::from_iter_instead_of_collect)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::single_match_else)]
#![allow(clippy::enum_glob_use)]
// TODO: delete these after writing docs
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::missing_errors_doc)]

pub mod core;
pub mod devserver;
pub mod discover;
pub mod render;
pub mod site_context;
pub mod util;

pub use render::Renderers;
pub use typed_path::*;

pub type Result<T> = eyre::Result<T>;

use thiserror::Error;

#[derive(Error, Debug)]
#[error(transparent)]
pub struct AsStdError(#[from] eyre::Report);

#[cfg(test)]
pub(crate) mod test {

    use std::sync::Arc;
    use tempfile::TempDir;
    use temptree::temptree;
    use typed_path::{ConfirmedPath, SysPath};

    use crate::{core::engine::EnginePaths, AbsPath, RelPath};

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

    pub fn default_test_paths(tree: &TempDir) -> Arc<EnginePaths> {
        Arc::new(EnginePaths {
            rule_script: RelPath::new("rules.rhai").unwrap(),
            src_dir: RelPath::new("src").unwrap(),
            syntax_theme_dir: RelPath::new("syntax_themes").unwrap(),
            output_dir: RelPath::new("target").unwrap(),
            template_dir: RelPath::new("templates").unwrap(),
            project_root: AbsPath::new(tree.path()).unwrap(),
        })
    }

    pub fn simple_init() -> (Arc<EnginePaths>, TempDir) {
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
        path.confirmed(pathmarker::MdFile)
            .expect("failed to make confirmed path")
    }
}
