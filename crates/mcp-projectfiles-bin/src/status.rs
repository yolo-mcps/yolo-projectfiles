use anyhow::Result;
use tabled::{Table, Tabled};
use tabled::settings::{Style, Modify, object::Columns, Alignment};
use crate::registration::{RegistrationManager, RegistrationLevel, get_claude_project_config_path};
use crate::permissions::PermissionsManager;

#[derive(Tabled)]
struct RegistrationRow {
    #[tabled(rename = "Level")]
    level: String,
    #[tabled(rename = "Status")]
    status: String,
    #[tabled(rename = "Config Location")]
    config_location: String,
}

#[derive(Tabled)]
struct PermissionsRow {
    #[tabled(rename = "Tool")]
    tool: String,
    #[tabled(rename = "Local")]
    local: String,
    #[tabled(rename = "User")]
    user: String,
    #[tabled(rename = "Project")]
    project: String,
}

pub fn show_claude_status_table() -> Result<()> {
    let manager = RegistrationManager::new(crate::registration::CLAUDE);
    let permissions_manager = PermissionsManager::new("projectfiles".to_string());
    
    println!("Claude Integration Status");
    println!("=========================");
    println!();
    
    // Check if project configuration exists under projects key
    let current_dir = std::env::current_dir()
        .map_err(|e| anyhow::anyhow!("Failed to get current directory: {}", e))?;
    let current_dir_str = current_dir.to_string_lossy().to_string();
    let config = manager.load_config(&RegistrationLevel::Local)?;
    let has_active_project_config = config.get("projects")
        .and_then(|projects| projects.get(&current_dir_str))
        .is_some();
    
    // Build registration table
    println!("Server Registration:");
    let mut registration_rows = Vec::new();
    
    // Local level
    if has_active_project_config {
        let is_registered = manager.is_server_registered(&RegistrationLevel::Local, "projectfiles")?;
        registration_rows.push(RegistrationRow {
            level: "Local".to_string(),
            status: if is_registered { "✓" } else { "✗" }.to_string(),
            config_location: format!("~/.claude.json (project: {})", current_dir.file_name().unwrap_or_default().to_string_lossy()),
        });
    } else {
        registration_rows.push(RegistrationRow {
            level: "Local".to_string(),
            status: "⚠".to_string(),
            config_location: "Claude not used in this project".to_string(),
        });
    }
    
    // User level
    let is_registered = manager.is_server_registered(&RegistrationLevel::User, "projectfiles")?;
    registration_rows.push(RegistrationRow {
        level: "User".to_string(),
        status: if is_registered { "✓" } else { "✗" }.to_string(),
        config_location: "~/.claude.json (global)".to_string(),
    });
    
    // Project level
    let is_registered = manager.is_server_registered(&RegistrationLevel::Project, "projectfiles")?;
    let project_config_path = get_claude_project_config_path()?;
    registration_rows.push(RegistrationRow {
        level: "Project".to_string(),
        status: if is_registered { "✓" } else { "✗" }.to_string(),
        config_location: if project_config_path.exists() {
            ".mcp.json (exists)".to_string()
        } else {
            ".mcp.json (not found)".to_string()
        },
    });
    
    // Display registration table
    let table = Table::new(&registration_rows)
        .with(Style::modern())
        .with(Modify::new(Columns::first()).with(Alignment::left()))
        .with(Modify::new(Columns::new(1..)).with(Alignment::center()))
        .to_string();
    
    println!("{}", table);
    
    // Build permissions matrix table
    println!();
    println!("Tool Permissions Matrix:");
    
    // Get file paths for headers
    let local_path = permissions_manager.get_settings_path(&crate::permissions::PermissionLevel::Local)?;
    let user_path = permissions_manager.get_settings_path(&crate::permissions::PermissionLevel::User)?;
    let project_path = permissions_manager.get_settings_path(&crate::permissions::PermissionLevel::Project)?;
    
    // Build table data
    let tools = permissions_manager.get_available_tools();
    let mut permission_rows = Vec::new();
    
    // Add file names as first row
    permission_rows.push(PermissionsRow {
        tool: "".to_string(),
        local: if has_active_project_config {
            format!(".claude/{}", local_path.file_name().unwrap_or_default().to_string_lossy())
        } else {
            "-".to_string()
        },
        user: format!("~/.claude/{}", user_path.file_name().unwrap_or_default().to_string_lossy()),
        project: format!(".claude/{}", project_path.file_name().unwrap_or_default().to_string_lossy()),
    });
    
    for tool in &tools {
        // Extract just the tool name (e.g., "read" from "mcp__projectfiles__read")
        let tool_name = tool.split("__").nth(2).unwrap_or(&tool);
        
        // Check permissions at each level
        let local_perms = if has_active_project_config {
            check_tool_permission(&permissions_manager, &crate::permissions::PermissionLevel::Local, &tool)?
        } else {
            "-".to_string()
        };
        
        let user_perms = check_tool_permission(&permissions_manager, &crate::permissions::PermissionLevel::User, &tool)?;
        let project_perms = check_tool_permission(&permissions_manager, &crate::permissions::PermissionLevel::Project, &tool)?;
        
        permission_rows.push(PermissionsRow {
            tool: tool_name.to_string(),
            local: local_perms,
            user: user_perms,
            project: project_perms,
        });
    }
    
    // Create table 
    let table = Table::new(&permission_rows)
        .with(Style::modern())
        .with(Modify::new(Columns::first()).with(Alignment::left()))
        .with(Modify::new(Columns::new(1..)).with(Alignment::center()))
        .to_string();
    
    println!("{}", table);
    
    println!();
    println!("Commands:");
    println!("  • Configure all settings: mcp-projectfiles claude configure");
    println!("  • Manage registrations only: mcp-projectfiles claude register");
    println!("  • Manage permissions only: mcp-projectfiles claude permissions");
    
    Ok(())
}

fn check_tool_permission(manager: &PermissionsManager, level: &crate::permissions::PermissionLevel, tool: &str) -> Result<String> {
    let (allowed, _denied) = manager.get_tool_permissions(level)?;
    
    if allowed.contains(&tool.to_string()) {
        Ok("✓".to_string())
    } else {
        Ok("-".to_string())
    }
}