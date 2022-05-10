pub mod uri;

pub use uri::{CheckedUri, Uri};

pub type Result<T> = eyre::Result<T>;
