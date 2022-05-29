use std::fmt;

// mod checked;
pub mod marker;
mod unchecked;

// pub use checked::*;
use eyre::eyre;
pub use marker::PathMarker;
use serde::Serialize;
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

    use std::ffi::OsStr;

    pub(in crate) use abs;
    pub(in crate) use rel;
}

#[derive(Debug, Clone, Serialize, Hash, PartialEq)]
pub struct TypedPath<T: PathMarker> {
    inner: SysPath,
    marker: T,
}

impl<T: PathMarker> TypedPath<T> {
    pub fn new(inner: &SysPath, marker: T) -> TypedPath<T> {
        Self {
            inner: inner.clone(),
            marker,
        }
    }

    pub fn confirm(&self) -> Result<ConfirmedPath<T>> {
        dbg!(&self.inner);
        match self.marker.confirm(self) {
            Ok(true) => Ok(ConfirmedPath {
                inner: (self.clone()),
            }),
            Ok(false) => Err(eyre!("failed to confirm path")),
            Err(e) => Err(e),
        }
    }

    pub fn as_sys_path(&self) -> &SysPath {
        &self.inner
    }

    pub fn marker(&self) -> &T {
        &self.marker
    }
}

impl<T: PathMarker + fmt::Display> fmt::Display for TypedPath<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TypedPath<{}>({})", self.marker(), self.inner)
    }
}

#[derive(Debug, Clone, Serialize, Hash, PartialEq)]
pub struct ConfirmedPath<T: PathMarker> {
    inner: TypedPath<T>,
}

impl<T: PathMarker> ConfirmedPath<T> {
    pub fn as_typed_path(&self) -> &TypedPath<T> {
        &self.inner
    }
    pub fn as_sys_path(&self) -> &SysPath {
        self.inner.as_sys_path()
    }
    pub fn marker(&self) -> &T {
        self.inner.marker()
    }
}

impl<T: PathMarker + fmt::Display> fmt::Display for ConfirmedPath<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ConfirmedPath<{}>({})", self.marker(), self.inner)
    }
}
