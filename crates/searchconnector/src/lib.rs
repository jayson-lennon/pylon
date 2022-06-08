pub mod provider;

pub type Result<T> = eyre::Result<T>;

pub use provider::{meilisearch, Meilisearch};
