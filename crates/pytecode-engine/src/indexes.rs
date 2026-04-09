//! Typed newtype wrappers for JVM constant pool and attribute indexes.
//!
//! Each type wraps a raw `u16` and exists purely for compile-time type safety.
//! All newtypes are `#[repr(transparent)]`, so they have zero runtime overhead.

use std::fmt;

macro_rules! define_index {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
        #[repr(transparent)]
        pub struct $name(pub u16);

        impl $name {
            #[inline]
            pub const fn new(value: u16) -> Self {
                Self(value)
            }

            #[inline]
            pub const fn value(self) -> u16 {
                self.0
            }
        }

        impl From<u16> for $name {
            #[inline]
            fn from(value: u16) -> Self {
                Self(value)
            }
        }

        impl From<$name> for u16 {
            #[inline]
            fn from(index: $name) -> u16 {
                index.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

define_index!(
    /// Generic constant pool index — points to any CP entry.
    CpIndex
);

define_index!(
    /// Index into a CONSTANT_Utf8 entry.
    Utf8Index
);

define_index!(
    /// Index into a CONSTANT_Class entry.
    ClassIndex
);

define_index!(
    /// Index into a CONSTANT_NameAndType entry.
    NameAndTypeIndex
);

define_index!(
    /// Index into a CONSTANT_Fieldref entry.
    FieldRefIndex
);

define_index!(
    /// Index into a CONSTANT_Methodref or CONSTANT_InterfaceMethodref entry.
    MethodRefIndex
);

define_index!(
    /// Index into a CONSTANT_Module entry.
    ModuleIndex
);

define_index!(
    /// Index into a CONSTANT_Package entry.
    PackageIndex
);

define_index!(
    /// Index into the BootstrapMethods attribute array (NOT a constant pool index).
    BootstrapMethodIndex
);

// Convenience conversions: any specific CP index can widen to CpIndex
macro_rules! impl_into_cp_index {
    ($($t:ty),+) => {
        $(
            impl From<$t> for CpIndex {
                #[inline]
                fn from(idx: $t) -> Self {
                    CpIndex(idx.0)
                }
            }
        )+
    };
}

impl_into_cp_index!(
    Utf8Index,
    ClassIndex,
    NameAndTypeIndex,
    FieldRefIndex,
    MethodRefIndex,
    ModuleIndex,
    PackageIndex
);

// When the `pyo3` feature is enabled, allow typed indexes to be passed directly
// to Python as integers without explicit `.value()` at every call site.
#[cfg(feature = "pyo3")]
macro_rules! impl_into_pyobject {
    ($($ty:ty),+) => {
        $(
            impl<'py> pyo3::IntoPyObject<'py> for $ty {
                type Target = pyo3::types::PyInt;
                type Output = pyo3::Bound<'py, pyo3::types::PyInt>;
                type Error = std::convert::Infallible;
                fn into_pyobject(self, py: pyo3::Python<'py>) -> Result<Self::Output, Self::Error> {
                    self.value().into_pyobject(py)
                }
            }
            impl<'py> pyo3::IntoPyObject<'py> for &$ty {
                type Target = pyo3::types::PyInt;
                type Output = pyo3::Bound<'py, pyo3::types::PyInt>;
                type Error = std::convert::Infallible;
                fn into_pyobject(self, py: pyo3::Python<'py>) -> Result<Self::Output, Self::Error> {
                    self.value().into_pyobject(py)
                }
            }
        )+
    };
}

#[cfg(feature = "pyo3")]
impl_into_pyobject!(
    CpIndex,
    Utf8Index,
    ClassIndex,
    NameAndTypeIndex,
    FieldRefIndex,
    MethodRefIndex,
    ModuleIndex,
    PackageIndex,
    BootstrapMethodIndex
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip() {
        let idx = Utf8Index::new(42);
        assert_eq!(idx.value(), 42);
        let raw: u16 = idx.into();
        assert_eq!(raw, 42);
        let back: Utf8Index = raw.into();
        assert_eq!(back, idx);
    }

    #[test]
    fn test_widen_to_cp_index() {
        let class_idx = ClassIndex::new(10);
        let cp: CpIndex = class_idx.into();
        assert_eq!(cp.value(), 10);
    }

    #[test]
    fn test_default_is_zero() {
        assert_eq!(CpIndex::default().value(), 0);
        assert_eq!(Utf8Index::default().value(), 0);
        assert_eq!(ClassIndex::default().value(), 0);
    }

    #[test]
    fn test_debug_display() {
        let idx = ClassIndex::new(7);
        assert_eq!(format!("{idx:?}"), "ClassIndex(7)");
        assert_eq!(format!("{idx}"), "7");
    }
}
