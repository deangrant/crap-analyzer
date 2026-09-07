//! Typed errors for I/O, coverage input, target discovery, and collection.

use std::fmt;
use std::io;
use std::path::PathBuf;

/// Failure that stops a run before a complete report.
#[derive(Debug)]
pub enum Error {
    /// A filesystem read or walk failed.
    Io {
        /// Path that could not be used.
        path: PathBuf,
        /// Underlying operating-system error.
        source: io::Error,
    },
    /// LCOV content is empty or has no valid line-hit records.
    Coverage(String),
    /// Target discovery failed.
    Resolve(String),
    /// Source collection failed (one or more files).
    Collect(String),
}

/// Result alias for crate operations.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Builds an I/O error for `path`.
    #[must_use]
    pub fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    /// Builds a coverage-input error from `message`.
    #[must_use]
    pub fn coverage(message: impl Into<String>) -> Self {
        Self::Coverage(message.into())
    }

    /// Builds a target-discovery error from `message`.
    #[must_use]
    pub fn resolve(message: impl Into<String>) -> Self {
        Self::Resolve(message.into())
    }

    /// Builds a collect error from `message`.
    #[must_use]
    pub fn collect(message: impl Into<String>) -> Self {
        Self::Collect(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Coverage(message) | Self::Resolve(message) | Self::Collect(message) => {
                write!(f, "{message}")
            }
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Coverage(_) | Self::Resolve(_) | Self::Collect(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as StdError;

    #[test]
    fn source_is_set_only_for_io() {
        let io = Error::io("x", io::Error::other("e"));
        assert!(StdError::source(&io).is_some());
        assert!(StdError::source(&Error::coverage("c")).is_none());
        assert!(StdError::source(&Error::resolve("r")).is_none());
        assert!(StdError::source(&Error::collect("p")).is_none());
    }

    #[test]
    fn display_covers_every_variant() {
        assert_eq!(Error::coverage("c").to_string(), "c");
        assert_eq!(Error::resolve("r").to_string(), "r");
        assert_eq!(Error::collect("p").to_string(), "p");
        assert!(Error::io("x", io::Error::other("e")).to_string().contains('x'));
    }
}
