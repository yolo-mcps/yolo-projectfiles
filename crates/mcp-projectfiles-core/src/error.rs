use thiserror::Error;

/// Structured error types for the MCP protocol implementation
#[derive(Debug, Error)]
pub enum Error {
    /// Transport layer errors
    #[error("Transport error: {message}")]
    Transport {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Protocol-level errors (invalid messages, version mismatches, etc.)
    #[error("Protocol error: {message}")]
    Protocol {
        message: String,
        error_code: Option<i32>,
    },

    /// Tool execution errors with context
    #[error("Tool execution error in '{tool_name}': {message}")]
    ToolExecution {
        tool_name: String,
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Validation errors for inputs
    #[error("Validation error: {message}")]
    Validation {
        message: String,
        field: Option<String>,
    },

    /// Configuration errors
    #[error("Configuration error: {message}")]
    Configuration { message: String },

    /// JSON serialization/deserialization errors
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// File system I/O errors
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Generic unknown errors
    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl Error {
    /// Create a new transport error with context
    pub fn transport<S: Into<String>>(message: S) -> Self {
        Self::Transport {
            message: message.into(),
            source: None,
        }
    }

    /// Create a new transport error with source
    pub fn transport_with_source<S: Into<String>>(
        message: S,
        source: Box<dyn std::error::Error + Send + Sync>,
    ) -> Self {
        Self::Transport {
            message: message.into(),
            source: Some(source),
        }
    }

    /// Create a new protocol error
    pub fn protocol<S: Into<String>>(message: S) -> Self {
        Self::Protocol {
            message: message.into(),
            error_code: None,
        }
    }

    /// Create a new protocol error with error code
    pub fn protocol_with_code<S: Into<String>>(message: S, error_code: i32) -> Self {
        Self::Protocol {
            message: message.into(),
            error_code: Some(error_code),
        }
    }

    /// Create a new tool execution error
    pub fn tool_execution<S: Into<String>, T: Into<String>>(tool_name: T, message: S) -> Self {
        Self::ToolExecution {
            tool_name: tool_name.into(),
            message: message.into(),
            source: None,
        }
    }

    /// Create a new tool execution error with source
    pub fn tool_execution_with_source<S: Into<String>, T: Into<String>>(
        tool_name: T,
        message: S,
        source: Box<dyn std::error::Error + Send + Sync>,
    ) -> Self {
        Self::ToolExecution {
            tool_name: tool_name.into(),
            message: message.into(),
            source: Some(source),
        }
    }

    /// Create a new validation error
    pub fn validation<S: Into<String>>(message: S) -> Self {
        Self::Validation {
            message: message.into(),
            field: None,
        }
    }

    /// Create a new validation error with field context
    pub fn validation_field<S: Into<String>, F: Into<String>>(message: S, field: F) -> Self {
        Self::Validation {
            message: message.into(),
            field: Some(field.into()),
        }
    }

    /// Create a new configuration error
    pub fn configuration<S: Into<String>>(message: S) -> Self {
        Self::Configuration {
            message: message.into(),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;