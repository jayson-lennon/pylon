pub mod uri;

pub use uri::{CheckedUri, Uri};

pub type Result<T> = std::result::Result<T, anyhow::Error>;
