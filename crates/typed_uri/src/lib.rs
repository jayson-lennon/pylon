pub mod uri;

pub use uri::{AssetUri, Uri};

pub type Result<T> = eyre::Result<T>;

#[cfg(test)]
mod test {

    #![allow(warnings, unused)]
    macro_rules! abs {
        ($path:literal) => {{
            &typed_path::AbsPath::new($path).unwrap()
        }};
        ($path:expr) => {{
            &typed_path::AbsPath::new($path).unwrap()
        }};
    }

    macro_rules! rel {
        ($path:literal) => {{
            &typed_path::RelPath::new($path).unwrap()
        }};
        ($path:expr) => {{
            &typed_path::RelPath::new($path).unwrap()
        }};
    }

    pub(in crate) use abs;
    pub(in crate) use rel;
}
