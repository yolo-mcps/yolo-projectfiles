mod chmod;
mod copy;
mod delete;
mod diff;
mod edit;
mod exists;
mod file_type;
mod find;
mod grep;
mod hash;
mod list;
mod mkdir;
mod move_file;
mod process;
mod read;
mod stat;
mod touch;
mod tree;
mod utils;
mod wc;
mod write;

use rust_mcp_sdk::tool_box;

pub use chmod::ChmodTool;
pub use copy::CopyTool;
pub use delete::DeleteTool;
pub use diff::DiffTool;
pub use edit::{EditTool, EditOperation};
pub use exists::ExistsTool;
pub use file_type::FileTypeTool;
pub use find::FindTool;
pub use grep::GrepTool;
pub use hash::HashTool;
pub use list::ListTool;
pub use mkdir::MkdirTool;
pub use move_file::MoveTool;
pub use process::ProcessTool;
pub use read::ReadTool;
pub use stat::StatTool;
pub use touch::TouchTool;
pub use tree::TreeTool;
pub use wc::WcTool;
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
        ChmodTool,
        GrepTool,
        ExistsTool,
        StatTool,
        DiffTool,
        FindTool,
        TreeTool,
        FileTypeTool,
        WcTool,
        HashTool,
        ProcessTool
    ]
);