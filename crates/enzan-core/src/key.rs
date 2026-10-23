//! The shape of a name the game gives: a stat, a source of modifiers, a class
//! of base.
//!
//! One rule serves all of them, so a name reads the same in data, logs and code
//! whatever it names. A type that holds such a name is made by `key_type!`; the
//! rule is stated once, on [`KeyError`].

/// Why a name could not be built.
///
/// A key is 1 to 64 bytes. The first character is `a-z`, and the rest are
/// `a-z`, `0-9` or `_`. Only ASCII is allowed, so a byte is a character.
///
/// The checks run in this order and the first failure is returned: empty,
/// too long, invalid start, invalid character (the byte position of the first
/// one).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum KeyError {
    /// The key is empty.
    #[error("a key must not be empty")]
    Empty,
    /// The key is longer than 64 bytes.
    #[error("a key must be at most 64 bytes")]
    TooLong,
    /// The first character is not `a-z`.
    #[error("a key must start with a-z")]
    InvalidStart,
    /// A character after the first is not `a-z`, `0-9` or `_`.
    #[error("a key may contain only a-z, 0-9 and _ (byte {index})")]
    InvalidChar {
        /// The byte position of the first invalid character.
        index: usize,
    },
}

const MAX_LEN: usize = 64;

/// Checks `s` against the rule in the doc of [`KeyError`].
pub(crate) fn validate(s: &str) -> Result<(), KeyError> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return Err(KeyError::Empty);
    }
    if bytes.len() > MAX_LEN {
        return Err(KeyError::TooLong);
    }
    if !bytes[0].is_ascii_lowercase() {
        return Err(KeyError::InvalidStart);
    }
    match bytes
        .iter()
        .position(|b| !(b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_'))
    {
        Some(index) => Err(KeyError::InvalidChar { index }),
        None => Ok(()),
    }
}

/// Declares a newtype over `String` whose value passes [`validate`].
macro_rules! key_type {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            /// The key as a string slice.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<&str> for $name {
            type Error = $crate::KeyError;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                $crate::key::validate(value)?;
                Ok(Self(value.to_owned()))
            }
        }

        impl TryFrom<String> for $name {
            type Error = $crate::KeyError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                $crate::key::validate(&value)?;
                Ok(Self(value))
            }
        }

        impl ::core::fmt::Display for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

pub(crate) use key_type;
