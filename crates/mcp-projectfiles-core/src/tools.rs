mod calculator;
mod counter;
mod file;
mod system;
mod time;

use rust_mcp_sdk::tool_box;

pub use calculator::CalculatorTool;
pub use counter::{CacheGetTool, CacheSetTool, CounterGetTool, CounterIncrementTool};
pub use file::{FileListTool, FileReadTool, FileWriteTool};
pub use system::{EnvironmentTool, SystemInfoTool};
pub use time::{CurrentTimeTool, TimestampTool};

tool_box!(
    ProtocolTools,
    [
        CalculatorTool,
        CounterIncrementTool,
        CounterGetTool,
        CacheSetTool,
        CacheGetTool,
        FileReadTool,
        FileWriteTool,
        FileListTool,
        SystemInfoTool,
        EnvironmentTool,
        CurrentTimeTool,
        TimestampTool
    ]
);