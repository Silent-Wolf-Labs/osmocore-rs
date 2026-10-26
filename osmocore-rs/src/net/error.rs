// SPDX-License-Identifier: GPL-3.0-or-later

use std::{fmt, io};

/// Establishment phase. The original I/O error (including errno) is retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Operation {
    Validate,
    Resolve,
    Create,
    Configure,
    Bind,
    Listen,
    Connect,
    SetNonblocking,
}
impl Operation {
    /// Stable diagnostic field value for this phase.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Validate => "validate",
            Self::Resolve => "resolve",
            Self::Create => "create",
            Self::Configure => "configure",
            Self::Bind => "bind",
            Self::Listen => "listen",
            Self::Connect => "connect",
            Self::SetNonblocking => "set_nonblocking",
        }
    }
}

/// Socket establishment error with its failing phase and original OS error.
#[derive(Debug)]
pub struct NetError {
    operation: Operation,
    source: io::Error,
}
impl NetError {
    pub fn operation(&self) -> Operation {
        self.operation
    }
    pub fn io_error(&self) -> &io::Error {
        &self.source
    }
    pub fn into_io_error(self) -> io::Error {
        self.source
    }
    pub(super) fn invalid(message: &'static str) -> Self {
        Self {
            operation: Operation::Validate,
            source: io::Error::new(io::ErrorKind::InvalidInput, message),
        }
    }
}
impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.operation.as_str(), self.source)
    }
}
impl std::error::Error for NetError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}
pub(super) fn phase<T>(operation: Operation, result: io::Result<T>) -> Result<T, NetError> {
    result.map_err(|source| NetError { operation, source })
}
