mod checked;
mod unchecked;

pub use checked::*;
pub use unchecked::*;

pub type Result<T> = eyre::Result<T>;

pub(in crate) mod helper {
    macro_rules! impl_try_from {
        ($src:ident => $target:ident) => {
            impl TryFrom<$src> for $target {
                type Error = eyre::Report;
                fn try_from(path: $src) -> Result<Self> {
                    Self::new(path)
                }
            }
        };
        (&$src:ident => $target:ident) => {
            impl TryFrom<&$src> for $target {
                type Error = eyre::Report;
                fn try_from(path: &$src) -> Result<Self> {
                    Self::new(path)
                }
            }
        };
    }
    pub(in crate) use impl_try_from;
}

#[cfg(test)]
mod test {

    #![allow(warnings, unused)]
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

    pub(in crate) use abs;
    pub(in crate) use rel;
}
