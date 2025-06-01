use anyhow::Result;
use clap::Parser;
use std::io::IsTerminal;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

mod cli;
mod registration;
use cli::{Cli, Commands, ShowCommands, ClaudeCommands};
use registration::{RegistrationManager, McpServerConfig, CLAUDE, prompt_projectfiles_registration, get_claude_user_config_path, get_claude_project_config_path};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging with TTY detection for conditional ANSI colors
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("mcp_projectfiles_core=info,mcp_projectfiles_bin=info,info"));

    fmt()
        .with_ansi(std::io::stderr().is_terminal()) // Only use ANSI colors if stderr is a TTY
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Stdio { name: _, version: _ } => {
            info!("Starting MCP server with stdio transport");
            mcp_projectfiles_core::run_stdio_server().await
        }
        Commands::Test => mcp_projectfiles_core::test_handler().await,
        Commands::Show { command } => match command {
            ShowCommands::Tools { name } => {
                list_tools(&name);
                Ok(())
            }
        },
        Commands::Claude { command } => match command {
            ClaudeCommands::Register => {
                register_claude()
            }
            ClaudeCommands::Status => {
                show_claude_status()
            }
        },
    }
}

fn list_tools(name: &str) {
    let tools = mcp_projectfiles_core::tools::ProtocolTools::tools();

    for tool in tools {
        println!("mcp__{}__{}", name, tool.name);
    }
}

fn register_claude() -> Result<()> {
    let manager = RegistrationManager::new(CLAUDE);
    
    println!("Register projectfiles MCP server with Claude");
    println!();

    // Check if we're in a TTY environment
    if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        println!("This command requires an interactive terminal.");
        println!("Current registration status:");
        
        // Check if project configuration exists under projects key
        let current_dir = std::env::current_dir()
            .map_err(|e| anyhow::anyhow!("Failed to get current directory: {}", e))?;
        let current_dir_str = current_dir.to_string_lossy().to_string();
        let config = manager.load_config(&registration::RegistrationLevel::Local)?;
        let has_active_project_config = config.get("projects")
            .and_then(|projects| projects.get(&current_dir_str))
            .is_some();
        
        // Show current status for available levels
        let mut levels = Vec::new();
        if has_active_project_config {
            levels.push(registration::RegistrationLevel::Local);
        }
        levels.push(registration::RegistrationLevel::User);
        levels.push(registration::RegistrationLevel::Project);
        
        for level in levels {
            let is_registered = manager.is_server_registered(&level, "projectfiles")?;
            let status = if is_registered { "✓ Registered" } else { "✗ Not registered" };
            println!("  {}: {}", level, status);
        }
        
        if !has_active_project_config {
            println!("  Note: Local level not available (Claude has not been used in this project)");
        }
        
        println!("\nTo register interactively, run this command from a terminal.");
        return Ok(());
    }

    // Show current status and prompt for changes
    let changes = prompt_projectfiles_registration(&manager)?;
    
    if changes.is_empty() {
        println!("No changes made.");
        return Ok(());
    }
    
    // Apply changes
    let config = McpServerConfig::new_stdio();
    let mut any_project_changes = false;
    
    for (level, should_register) in changes {
        if should_register {
            // Register
            manager.register_server(&level, "projectfiles", &config)?;
            println!("✓ projectfiles registered at {} level", level);
            if level == registration::RegistrationLevel::Project {
                any_project_changes = true;
            }
        } else {
            // Unregister
            manager.unregister_server(&level, "projectfiles")?;
            println!("✗ projectfiles unregistered from {} level", level);
            if level == registration::RegistrationLevel::Project {
                any_project_changes = true;
            }
        }
    }
    
    // Show appropriate restart notice
    if any_project_changes {
        println!("Configuration updated in .mcp.json");
    } else {
        println!("Note: You may need to restart Claude for changes to take effect.");
    }
    
    Ok(())
}

fn show_claude_status() -> Result<()> {
    let manager = RegistrationManager::new(CLAUDE);
    
    println!("Claude Integration Status");
    println!("=========================");
    println!();
    println!("Registration Levels:");
    println!();
    
    // Check if project configuration exists under projects key
    let current_dir = std::env::current_dir()
        .map_err(|e| anyhow::anyhow!("Failed to get current directory: {}", e))?;
    let current_dir_str = current_dir.to_string_lossy().to_string();
    let config = manager.load_config(&registration::RegistrationLevel::Local)?;
    let has_active_project_config = config.get("projects")
        .and_then(|projects| projects.get(&current_dir_str))
        .is_some();
    
    // Check Local level (project-specific in ~/.claude.json)
    if has_active_project_config {
        let is_registered = manager.is_server_registered(&registration::RegistrationLevel::Local, "projectfiles")?;
        let status = if is_registered { "✓ Registered" } else { "✗ Not registered" };
        println!("  Local:   {} (project-specific in ~/.claude.json)", status);
    } else {
        println!("  Local:   ⚠ Not available (Claude has not been used in this project)");
    }
    
    // Check User level (global in ~/.claude.json)
    let is_registered = manager.is_server_registered(&registration::RegistrationLevel::User, "projectfiles")?;
    let status = if is_registered { "✓ Registered" } else { "✗ Not registered" };
    println!("  User:    {} (global in ~/.claude.json)", status);
    
    // Check Project level (.mcp.json file)
    let is_registered = manager.is_server_registered(&registration::RegistrationLevel::Project, "projectfiles")?;
    let status = if is_registered { "✓ Registered" } else { "✗ Not registered" };
    println!("  Project: {} (in .mcp.json)", status);
    
    println!();
    println!("Configuration Files:");
    println!();
    
    // Show configuration file paths
    let claude_config_path = get_claude_user_config_path()?;
    println!("  Claude config: {}", claude_config_path.display());
    
    let project_config_path = get_claude_project_config_path()?;
    if project_config_path.exists() {
        println!("  Project config: {} (exists)", project_config_path.display());
    } else {
        println!("  Project config: {} (not found)", project_config_path.display());
    }
    
    println!();
    println!("To manage registrations, run: mcp-projectfiles claude register");
    
    Ok(())
}