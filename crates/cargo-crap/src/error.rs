//! Typed errors for usage, I/O, and analysis failures.

use std::fmt;
use std::io;
use std::path::PathBuf;

/// Failure that stops a run before a complete report.
#[derive(Debug)]
pub enum Error {
    /// Invalid flags or missing required values.
    Usage(String),
    /// A filesystem read or walk failed.
    Io {
        /// Path that could not be used.
        path: PathBuf,
        /// Underlying operating-system error.
        source: io::Error,
    },
    /// `cargo metadata` failed or returned an unexpected shape.
    Metadata(String),
    /// A `-p` name is not a workspace member.
    UnknownPackage(String),
    /// A Rust source file could not be parsed.
    Parse(String),
}

/// Result alias for crate operations.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Builds a usage error from `message`.
    #[must_use]
    pub fn usage(message: impl Into<String>) -> Self {
        Self::Usage(message.into())
    }

    /// Builds an I/O error for `path`.
    #[must_use]
    pub fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) | Self::Parse(message) => write!(f, "{message}"),
            Self::Io { path, source } => {
                write!(f, "{}: {source}", path.display())
            }
            Self::Metadata(message) => write!(f, "cargo metadata: {message}"),
            Self::UnknownPackage(name) => {
                write!(f, "unknown package `{name}`")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Usage(_) | Self::Metadata(_) | Self::UnknownPackage(_) | Self::Parse(_) => None,
        }
    }
}
