// SPDX-License-Identifier: GPL-3.0-or-later

//! Key types. Each wraps its bytes, and its `Debug` output doesn't show them.

use std::fmt;

macro_rules! fixed_key {
    ($(#[$doc:meta])* $name:ident, $len:expr) => {
        $(#[$doc])*
        #[derive(Clone, PartialEq, Eq)]
        pub struct $name([u8; $len]);

        impl $name {
            /// Wraps a key.
            pub const fn new(bytes: [u8; $len]) -> Self {
                Self(bytes)
            }

            /// Returns the key.
            pub const fn as_bytes(&self) -> &[u8; $len] {
                &self.0
            }
        }

        impl From<[u8; $len]> for $name {
            fn from(bytes: [u8; $len]) -> Self {
                Self(bytes)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($name), "(..)"))
            }
        }
    };
}

fixed_key!(
    /// A GSM subscriber's secret key.
    Ki,
    16
);
fixed_key!(
    /// A GSM cipher key, as A5/1 to A5/3 take it.
    Kc,
    8
);
fixed_key!(
    /// A UMTS cipher key.
    Ck,
    16
);
fixed_key!(
    /// A UMTS integrity key.
    Ik,
    16
);

/// The given key has a length other than 16 or 32 bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyLengthError(pub usize);

impl fmt::Display for KeyLengthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a key has 16 or 32 bytes, not {}", self.0)
    }
}

impl std::error::Error for KeyLengthError {}

macro_rules! sized_key {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, PartialEq, Eq)]
        pub struct $name {
            bytes: [u8; 32],
            len: u8,
        }

        impl $name {
            /// Wraps a 16 or 32-byte key.
            pub fn new(bytes: &[u8]) -> Result<Self, KeyLengthError> {
                if bytes.len() != 16 && bytes.len() != 32 {
                    return Err(KeyLengthError(bytes.len()));
                }
                let mut stored = [0; 32];
                stored[..bytes.len()].copy_from_slice(bytes);
                Ok(Self {
                    bytes: stored,
                    len: bytes.len() as u8,
                })
            }

            /// Returns the key.
            pub fn as_bytes(&self) -> &[u8] {
                &self.bytes[..usize::from(self.len)]
            }

            /// Returns the key if it has 16 bytes.
            pub fn as_128(&self) -> Option<&[u8; 16]> {
                self.as_bytes().try_into().ok()
            }
        }

        impl From<[u8; 16]> for $name {
            fn from(bytes: [u8; 16]) -> Self {
                Self::new(&bytes).expect("16 bytes")
            }
        }

        impl From<[u8; 32]> for $name {
            fn from(bytes: [u8; 32]) -> Self {
                Self::new(&bytes).expect("32 bytes")
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "({} bytes)"), self.len)
            }
        }
    };
}

sized_key!(
    /// A UMTS subscriber's secret key, of 16 or 32 bytes. Milenage and XOR-3G use 16.
    K
);
sized_key!(
    /// An operator variant key, of 16 or 32 bytes, from which each subscriber's OPc is
    /// derived.
    Op
);
sized_key!(
    /// An operator variant key already combined with the subscriber's K, of 16 or 32
    /// bytes.
    Opc
);
