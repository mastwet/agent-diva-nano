use std::fmt;

/// Errors that can occur when using the nano agent library.
#[derive(Debug)]
pub enum NanoError {
    /// The agent failed to process the request.
    Agent(String),
    /// A provider (LLM) error occurred.
    Provider(String),
    /// The request timed out.
    Timeout,
    /// An I/O or configuration error.
    Other(String),
}

impl fmt::Display for NanoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NanoError::Agent(msg) => write!(f, "agent error: {msg}"),
            NanoError::Provider(msg) => write!(f, "provider error: {msg}"),
            NanoError::Timeout => write!(f, "request timed out"),
            NanoError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for NanoError {}

impl From<anyhow::Error> for NanoError {
    fn from(err: anyhow::Error) -> Self {
        NanoError::Other(err.to_string())
    }
}
