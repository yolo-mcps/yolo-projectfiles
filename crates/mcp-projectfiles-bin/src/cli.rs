use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "projectfiles")]
#[command(author, version, about = "MCP Protocol Server", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run the MCP server using stdio transport
    Stdio {
        /// Server name
        #[arg(long, default_value = "projectfiles")]
        name: String,

        /// Server version
        #[arg(long, default_value = env!("CARGO_PKG_VERSION"))]
        version: String,
    },
    /// Run the MCP server using SSE transport
    Sse {
        /// Server name
        #[arg(long, default_value = "projectfiles")]
        name: String,

        /// Server version
        #[arg(long, default_value = env!("CARGO_PKG_VERSION"))]
        version: String,

        /// Port to listen on
        #[arg(short, long, default_value = "3000")]
        port: u16,
    },
    /// Test the tool handler implementation
    Test,
    /// Show various MCP server components
    Show {
        #[command(subcommand)]
        command: ShowCommands,
    },
}

#[derive(Subcommand)]
pub enum ShowCommands {
    /// Show all available tools with their MCP naming convention
    Tools {
        /// Custom name to replace 'projectfiles' in tool names
        #[arg(long, default_value = "projectfiles")]
        name: String,
    },
}