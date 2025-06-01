mod chmod;
mod copy;
mod delete;
mod edit;
mod list;
mod mkdir;
mod move_file;
mod read;
mod touch;
mod write;

use rust_mcp_sdk::tool_box;

pub use chmod::ChmodTool;
pub use copy::CopyTool;
pub use delete::DeleteTool;
pub use edit::{EditTool, EditOperation};
pub use list::ListTool;
pub use mkdir::MkdirTool;
pub use move_file::MoveTool;
pub use read::ReadTool;
pub use touch::TouchTool;
pub use write::WriteTool;

tool_box!(
    ProtocolTools,
    [
        ReadTool,
        WriteTool,
        EditTool,
        ListTool,
        MoveTool,
        CopyTool,
        DeleteTool,
        MkdirTool,
        TouchTool,
        ChmodTool
    ]
);