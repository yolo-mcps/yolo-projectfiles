use anyhow::{anyhow, Result};
use inquire::{MultiSelect, Select};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use dirs;
use mcp_projectfiles_core::tools::ProtocolTools;

#[derive(Debug, Clone, PartialEq)]
pub enum PermissionLevel {
    Local,
    Project,
    User,
}

impl std::fmt::Display for PermissionLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PermissionLevel::Local => write!(f, "Local (project-specific, not checked in)"),
            PermissionLevel::Project => write!(f, "Project (checked into repository)"),
            PermissionLevel::User => write!(f, "User (global)"),
        }
    }
}

pub struct PermissionsManager {
    server_name: String,
}

impl PermissionsManager {
    pub fn new(server_name: String) -> Self {
        Self { server_name }
    }

    pub fn get_settings_path(&self, level: &PermissionLevel) -> Result<PathBuf> {
        match level {
            PermissionLevel::Local => {
                let current_dir = std::env::current_dir()
                    .map_err(|e| anyhow!("Failed to get current directory: {}", e))?;
                Ok(current_dir.join(".claude").join("settings.local.json"))
            }
            PermissionLevel::Project => {
                let current_dir = std::env::current_dir()
                    .map_err(|e| anyhow!("Failed to get current directory: {}", e))?;
                Ok(current_dir.join(".claude").join("settings.json"))
            }
            PermissionLevel::User => {
                let home_dir = dirs::home_dir()
                    .ok_or_else(|| anyhow!("Could not find home directory"))?;
                Ok(home_dir.join(".claude").join("settings.json"))
            }
        }
    }

    pub fn load_settings(&self, level: &PermissionLevel) -> Result<Value> {
        let path = self.get_settings_path(level)?;
        
        if !path.exists() {
            return Ok(json!({
                "permissions": {
                    "allow": [],
                    "deny": []
                }
            }));
        }

        let content = fs::read_to_string(&path)
            .map_err(|e| anyhow!("Failed to read settings file at {}: {}", path.display(), e))?;
        
        let settings: Value = serde_json::from_str(&content)
            .map_err(|e| anyhow!("Failed to parse settings file: {}", e))?;
        
        Ok(settings)
    }

    pub fn save_settings(&self, level: &PermissionLevel, settings: &Value) -> Result<()> {
        let path = self.get_settings_path(level)?;
        
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| anyhow!("Failed to create settings directory: {}", e))?;
        }

        let content = serde_json::to_string_pretty(settings)
            .map_err(|e| anyhow!("Failed to serialize settings: {}", e))?;
        
        fs::write(&path, content)
            .map_err(|e| anyhow!("Failed to write settings file: {}", e))?;
        
        Ok(())
    }

    pub fn get_available_tools(&self) -> Vec<String> {
        // Dynamically get tools from the tool registry
        ProtocolTools::tools()
            .into_iter()
            .map(|tool| tool.name)
            .collect()
    }

    pub fn get_tool_permissions(&self, level: &PermissionLevel) -> Result<(Vec<String>, Vec<String>)> {
        let settings = self.load_settings(level)?;
        
        let allow = settings.get("permissions")
            .and_then(|p| p.get("allow"))
            .and_then(|a| a.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();
            
        let deny = settings.get("permissions")
            .and_then(|p| p.get("deny"))
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();
            
        Ok((allow, deny))
    }

    pub fn update_tool_permissions(&self, level: &PermissionLevel, allowed_tools: Vec<String>) -> Result<()> {
        let mut settings = self.load_settings(level)?;
        
        // Ensure permissions structure exists
        if settings.get("permissions").is_none() {
            settings["permissions"] = json!({
                "allow": [],
                "deny": []
            });
        }
        
        // Get available tools for this server
        let available_tools = self.get_available_tools();
        let available_set: HashSet<_> = available_tools.iter().collect();
        
        // Filter allowed tools to only include those that exist
        let filtered_allowed: Vec<String> = allowed_tools.into_iter()
            .filter(|tool| available_set.contains(tool))
            .collect();
        
        // Format tools with MCP prefix pattern: mcp__server_name__tool_name
        let mcp_formatted_tools: Vec<String> = filtered_allowed.into_iter()
            .map(|tool| format!("mcp__{}__{}", self.server_name, tool))
            .collect();
        
        // Get existing permissions
        let (mut all_allow, all_deny) = self.get_tool_permissions(level)?;
        
        // Remove all tools for this server from the allow list
        all_allow.retain(|tool| !tool.starts_with(&format!("mcp__{}_", self.server_name)));
        
        // Add the newly allowed tools with MCP prefix
        all_allow.extend(mcp_formatted_tools);
        
        // Update the settings
        settings["permissions"]["allow"] = json!(all_allow);
        settings["permissions"]["deny"] = json!(all_deny);
        
        self.save_settings(level, &settings)
    }
}

pub fn prompt_permission_level() -> Result<PermissionLevel> {
    let levels = vec![
        PermissionLevel::Local,
        PermissionLevel::Project,
        PermissionLevel::User,
    ];
    
    let selection = Select::new("Select permission level to manage:", levels)
        .prompt()
        .map_err(|_| anyhow!("User cancelled selection"))?;
    
    Ok(selection)
}

pub fn prompt_tool_permissions(manager: &PermissionsManager, level: &PermissionLevel) -> Result<Vec<String>> {
    let available_tools = manager.get_available_tools();
    let (allowed, _) = manager.get_tool_permissions(level)?;
    
    // Find which tools are currently allowed
    let mut default_indices = Vec::new();
    for (i, tool) in available_tools.iter().enumerate() {
        let mcp_tool_name = format!("mcp__{}__{}", manager.server_name, tool);
        if allowed.contains(&mcp_tool_name) {
            default_indices.push(i);
        }
    }
    
    // Create tool descriptions - just show the tool names without MCP prefix
    let tool_descriptions: Vec<String> = available_tools.clone();
    
    let result = MultiSelect::new("Select tools to allow:", tool_descriptions)
        .with_default(&default_indices)
        .prompt()
        .map_err(|_| anyhow!("User cancelled selection"))?;
    
    // Return the selected tool names (without MCP prefix)
    Ok(result)
}

pub fn manage_permissions(server_name: String) -> Result<()> {
    let manager = PermissionsManager::new(server_name);
    
    // Prompt for permission level
    let level = prompt_permission_level()?;
    
    // Check if settings file exists and show current status
    let settings_path = manager.get_settings_path(&level)?;
    if !settings_path.exists() {
        println!("\nNote: Settings file does not exist at {}", settings_path.display());
        println!("It will be created when you save permissions.");
    }
    
    // Prompt for tool permissions
    let selected_tools = prompt_tool_permissions(&manager, &level)?;
    
    // Update permissions
    manager.update_tool_permissions(&level, selected_tools)?;
    
    println!("\n✓ Permissions updated successfully at {} level", level);
    println!("Settings saved to: {}", settings_path.display());
    
    Ok(())
}