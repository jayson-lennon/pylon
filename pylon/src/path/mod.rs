mod asset;
mod checked;
mod unchecked;
mod uri;

pub use asset::AssetPath;
pub use checked::*;
pub use unchecked::*;
pub use uri::{CheckedUri, Uri};

pub(in crate::path) mod helper {
    macro_rules! impl_try_from {
        ($src:ident => $target:ident) => {
            impl TryFrom<$src> for $target {
                type Error = anyhow::Error;
                fn try_from(path: $src) -> Result<Self> {
                    Self::new(path)
                }
            }
        };
        (&$src:ident => $target:ident) => {
            impl TryFrom<&$src> for $target {
                type Error = anyhow::Error;
                fn try_from(path: &$src) -> Result<Self> {
                    Self::new(path)
                }
            }
        };
    }
    pub(in crate::path) use impl_try_from;
}
