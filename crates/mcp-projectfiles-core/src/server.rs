// Re-export the modular handlers for users who want specific transport implementations
pub use crate::handler::CoreHandler;
pub use crate::transports::StdioHandler;

// Re-export the transport-specific server functions
pub use crate::transports::run_stdio_server;