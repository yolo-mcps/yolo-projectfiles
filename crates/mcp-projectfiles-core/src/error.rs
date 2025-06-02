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

    /// File or directory not found
    #[error("{server}:{tool} - File not found: {path}")]
    FileNotFound {
        server: String,
        tool: String,
        path: String,
    },

    /// Access denied - path outside project directory or insufficient permissions
    #[error("{server}:{tool} - Access denied: {path} ({reason})")]
    AccessDenied {
        server: String,
        tool: String,
        path: String,
        reason: String,
    },

    /// Invalid input parameters
    #[error("{server}:{tool} - {message}")]
    InvalidInput {
        server: String,
        tool: String,
        message: String,
    },

    /// Binary file detected when text was expected
    #[error("{server}:{tool} - Binary file detected: {path}")]
    BinaryFile {
        server: String,
        tool: String,
        path: String,
    },

    /// Regex or pattern compilation error
    #[error("{server}:{tool} - Invalid pattern '{pattern}': {message}")]
    PatternError {
        server: String,
        tool: String,
        pattern: String,
        message: String,
    },

    /// File encoding issues
    #[error("{server}:{tool} - Encoding error in '{path}' with {encoding}")]
    EncodingError {
        server: String,
        tool: String,
        path: String,
        encoding: String,
    },

    /// Operation not permitted (e.g., file not read before edit)
    #[error("{server}:{tool} - {message}")]
    OperationNotPermitted {
        server: String,
        tool: String,
        message: String,
    },

    /// Resource limits exceeded
    #[error("{server}:{tool} - Limit exceeded: {limit} (actual: {actual})")]
    LimitExceeded {
        server: String,
        tool: String,
        limit: String,
        actual: String,
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

    /// Create a file not found error
    pub fn file_not_found<S: Into<String>, T: Into<String>, P: Into<String>>(
        server: S, tool: T, path: P
    ) -> Self {
        Self::FileNotFound {
            server: server.into(),
            tool: tool.into(),
            path: path.into(),
        }
    }

    /// Create an access denied error
    pub fn access_denied<S: Into<String>, T: Into<String>, P: Into<String>, R: Into<String>>(
        server: S, tool: T, path: P, reason: R
    ) -> Self {
        Self::AccessDenied {
            server: server.into(),
            tool: tool.into(),
            path: path.into(),
            reason: reason.into(),
        }
    }

    /// Create an invalid input error
    pub fn invalid_input<S: Into<String>, T: Into<String>, M: Into<String>>(
        server: S, tool: T, message: M
    ) -> Self {
        Self::InvalidInput {
            server: server.into(),
            tool: tool.into(),
            message: message.into(),
        }
    }

    /// Create a binary file error
    pub fn binary_file<S: Into<String>, T: Into<String>, P: Into<String>>(
        server: S, tool: T, path: P
    ) -> Self {
        Self::BinaryFile {
            server: server.into(),
            tool: tool.into(),
            path: path.into(),
        }
    }

    /// Create a pattern error
    pub fn pattern_error<S: Into<String>, T: Into<String>, P: Into<String>, M: Into<String>>(
        server: S, tool: T, pattern: P, message: M
    ) -> Self {
        Self::PatternError {
            server: server.into(),
            tool: tool.into(),
            pattern: pattern.into(),
            message: message.into(),
        }
    }

    /// Create an encoding error
    pub fn encoding_error<S: Into<String>, T: Into<String>, P: Into<String>, E: Into<String>>(
        server: S, tool: T, path: P, encoding: E
    ) -> Self {
        Self::EncodingError {
            server: server.into(),
            tool: tool.into(),
            path: path.into(),
            encoding: encoding.into(),
        }
    }

    /// Create an operation not permitted error
    pub fn operation_not_permitted<S: Into<String>, T: Into<String>, M: Into<String>>(
        server: S, tool: T, message: M
    ) -> Self {
        Self::OperationNotPermitted {
            server: server.into(),
            tool: tool.into(),
            message: message.into(),
        }
    }

    /// Create a limit exceeded error
    pub fn limit_exceeded<S: Into<String>, T: Into<String>, L: Into<String>, A: Into<String>>(
        server: S, tool: T, limit: L, actual: A
    ) -> Self {
        Self::LimitExceeded {
            server: server.into(),
            tool: tool.into(),
            limit: limit.into(),
            actual: actual.into(),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Convert our Error type to CallToolError for MCP protocol responses
impl From<Error> for rust_mcp_schema::schema_utils::CallToolError {
    fn from(error: Error) -> Self {
        // Create an appropriate std::io::Error with the right ErrorKind
        let io_error = match &error {
            Error::FileNotFound { .. } => {
                std::io::Error::new(std::io::ErrorKind::NotFound, error.to_string())
            }
            Error::AccessDenied { .. } => {
                std::io::Error::new(std::io::ErrorKind::PermissionDenied, error.to_string())
            }
            Error::InvalidInput { .. } => {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, error.to_string())
            }
            Error::BinaryFile { .. } => {
                std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string())
            }
            Error::PatternError { .. } => {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, error.to_string())
            }
            Error::EncodingError { .. } => {
                std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string())
            }
            Error::OperationNotPermitted { .. } => {
                std::io::Error::new(std::io::ErrorKind::PermissionDenied, error.to_string())
            }
            Error::LimitExceeded { .. } => {
                std::io::Error::new(std::io::ErrorKind::Other, error.to_string())
            }
            Error::Io(io_err) => {
                std::io::Error::new(io_err.kind(), error.to_string())
            }
            _ => {
                std::io::Error::new(std::io::ErrorKind::Other, error.to_string())
            }
        };
        
        rust_mcp_schema::schema_utils::CallToolError::new(io_error)
    }
}