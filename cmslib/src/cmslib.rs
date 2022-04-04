#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::from_iter_instead_of_collect)]

pub mod core;
pub mod devserver;
pub mod discover;
pub mod frontmatter;
pub mod pipeline;
pub mod render;
pub mod site_context;
pub mod util;

pub use render::Renderers;
