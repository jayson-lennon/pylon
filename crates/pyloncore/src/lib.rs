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
pub mod pipeline;
pub mod render;
pub mod site_context;
pub mod util;

pub use render::Renderers;

pub type Result<T> = std::result::Result<T, anyhow::Error>;

use thiserror::Error;

#[derive(Error, Debug)]
#[error(transparent)]
pub struct AsStdError(#[from] anyhow::Error);
