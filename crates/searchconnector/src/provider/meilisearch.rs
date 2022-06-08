use futures::executor::block_on;
use meilisearch_sdk::{client::*, indexes::*, search::*, settings::*, tasks::Task};
use serde::{Deserialize, Serialize};
use std::{fs::File, io::prelude::*, net::SocketAddr};

use crate::Result;

#[derive(Debug)]
pub struct Meilisearch {
    addr: String,
    client: Client,
}

impl Meilisearch {
    pub fn new<A, K>(addr: A, api_key: K) -> Self
    where
        A: Into<String>,
        K: Into<String>,
    {
        let addr = addr.into();
        let client = Client::new(addr.to_string(), api_key);

        Self { addr, client }
    }

    fn index<I: Into<String>>(&self, name: I) -> Index {
        self.client.index(name)
    }

    pub async fn set_searchable_attributes<I, A>(
        &self,
        index_name: I,
        attributes: &[A],
    ) -> Result<Task>
    where
        I: Into<String>,
        A: AsRef<str>,
    {
        let index = self.index(index_name);
        Ok(index.set_searchable_attributes(attributes).await?)
    }

    pub async fn populate<I, D>(
        &self,
        index_name: I,
        docs: &[D],
        primary_key: Option<&str>,
    ) -> Result<Task>
    where
        I: Into<String>,
        D: Serialize,
    {
        // adding documents
        Ok(self
            .client
            .index(index_name)
            .add_documents(docs, primary_key)
            .await?)
    }
}
