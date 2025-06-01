// Re-export the modular handlers for users who want specific transport implementations
pub use crate::handler::CoreHandler;
pub use crate::transports::{SseHandler, StdioHandler};

// Re-export the transport-specific server functions
pub use crate::transports::{run_sse_server, run_stdio_server};