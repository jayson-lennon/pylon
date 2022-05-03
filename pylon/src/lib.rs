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
pub mod path;
pub mod pipeline;
pub mod render;
pub mod site_context;
pub mod util;

pub use path::*;
pub use render::Renderers;

pub type Result<T> = std::result::Result<T, anyhow::Error>;

use thiserror::Error;

#[derive(Error, Debug)]
#[error(transparent)]
pub struct AsStdError(#[from] anyhow::Error);

#[cfg(test)]
pub(crate) mod test {
    macro_rules! rel {
        ($path:literal) => {{
            &crate::RelPath::new($path).unwrap()
        }};
        ($path:expr) => {{
            &crate::RelPath::new($path).unwrap()
        }};
    }

    macro_rules! abs {
        ($path:literal) => {{
            &crate::AbsPath::new($path).unwrap()
        }};
        ($path:expr) => {{
            &crate::AbsPath::new($path).unwrap()
        }};
    }
    pub(crate) use abs;
    pub(crate) use rel;

    use std::sync::Arc;
    use tempfile::TempDir;
    use temptree::temptree;

    use crate::{core::engine::EnginePaths, AbsPath, RelPath};

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
}
