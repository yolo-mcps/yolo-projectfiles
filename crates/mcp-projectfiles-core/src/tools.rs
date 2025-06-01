mod list;
mod read;
mod write;

use rust_mcp_sdk::tool_box;

pub use list::ListTool;
pub use read::ReadTool;
pub use write::WriteTool;

tool_box!(
    ProtocolTools,
    [
        ReadTool,
        WriteTool,
        ListTool
    ]
);