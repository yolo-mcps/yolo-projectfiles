use anyhow::{anyhow, Result};
use dirs;
use inquire::MultiSelect;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum RegistrationLevel {
    Local,
    User,
    Project,
}

impl std::fmt::Display for RegistrationLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistrationLevel::Local => write!(f, "Local"),
            RegistrationLevel::User => write!(f, "User"),
            RegistrationLevel::Project => write!(f, "Project"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AssistantConfig {
    pub config_path_fn: fn() -> Result<PathBuf>,
    pub project_config_path_fn: fn() -> Result<PathBuf>,
}

pub const CLAUDE: AssistantConfig = AssistantConfig {
    config_path_fn: get_claude_user_config_path,
    project_config_path_fn: get_claude_project_config_path,
};

// Future assistant configurations can be added here
// pub const CURSOR: AssistantConfig = AssistantConfig {
//     config_path_fn: get_cursor_user_config_path,
//     project_config_path_fn: get_cursor_project_config_path,
// };

// pub const WINDSURF: AssistantConfig = AssistantConfig {
//     config_path_fn: get_windsurf_user_config_path,
//     project_config_path_fn: get_windsurf_project_config_path,
// };

#[derive(Debug, Clone)]
pub struct McpServerConfig {
    pub server_type: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}

impl McpServerConfig {
    pub fn new_stdio() -> Self {
        Self {
            server_type: "stdio".to_string(),
            command: "mcp-projectfiles".to_string(),
            args: vec!["stdio".to_string()],
            env: HashMap::new(),
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "type": self.server_type,
            "command": self.command,
            "args": self.args,
            "env": self.env
        })
    }
}

pub struct RegistrationManager {
    assistant: AssistantConfig,
}

impl RegistrationManager {
    pub fn new(assistant: AssistantConfig) -> Self {
        Self { assistant }
    }

    pub fn get_config_path(&self, level: &RegistrationLevel) -> Result<PathBuf> {
        match level {
            RegistrationLevel::Local | RegistrationLevel::User => (self.assistant.config_path_fn)(),
            RegistrationLevel::Project => (self.assistant.project_config_path_fn)(),
        }
    }

    pub fn load_config(&self, level: &RegistrationLevel) -> Result<Value> {
        let path = self.get_config_path(level)?;
        
        if !path.exists() {
            return Ok(json!({"mcpServers": {}}));
        }

        let content = fs::read_to_string(&path)
            .map_err(|e| anyhow!("Failed to read config file at {}: {}", path.display(), e))?;
        
        let config: Value = serde_json::from_str(&content)
            .map_err(|e| anyhow!("Failed to parse config file: {}", e))?;
        
        Ok(config)
    }

    pub fn save_config(&self, level: &RegistrationLevel, config: &Value) -> Result<()> {
        let path = self.get_config_path(level)?;
        
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| anyhow!("Failed to create config directory: {}", e))?;
        }

        let content = serde_json::to_string_pretty(config)
            .map_err(|e| anyhow!("Failed to serialize config: {}", e))?;
        
        fs::write(&path, content)
            .map_err(|e| anyhow!("Failed to write config file: {}", e))?;
        
        Ok(())
    }

    pub fn is_server_registered(&self, level: &RegistrationLevel, server_name: &str) -> Result<bool> {
        let config = self.load_config(level)?;
        
        let mcp_servers = match level {
            RegistrationLevel::User => {
                // Root-level mcpServers
                config.get("mcpServers")
            }
            RegistrationLevel::Local => {
                // Project-specific mcpServers under projects key
                let current_dir = std::env::current_dir()
                    .map_err(|e| anyhow!("Failed to get current directory: {}", e))?;
                let current_dir_str = current_dir.to_string_lossy().to_string();
                
                // Look in the projects section
                config.get("projects")
                    .and_then(|projects| projects.get(&current_dir_str))
                    .and_then(|project| project.get("mcpServers"))
            }
            RegistrationLevel::Project => {
                // .mcp.json file
                config.get("mcpServers")
            }
        };
        
        Ok(mcp_servers
            .and_then(|servers| servers.get(server_name))
            .is_some())
    }

    pub fn register_server(&self, level: &RegistrationLevel, server_name: &str, server_config: &McpServerConfig) -> Result<()> {
        let mut config = self.load_config(level)?;
        
        match level {
            RegistrationLevel::User => {
                // Root-level mcpServers
                if config.get("mcpServers").is_none() {
                    config["mcpServers"] = json!({});
                }
                config["mcpServers"][server_name] = server_config.to_json();
            }
            RegistrationLevel::Local => {
                // Project-specific mcpServers under projects key
                let current_dir = std::env::current_dir()
                    .map_err(|e| anyhow!("Failed to get current directory: {}", e))?;
                let current_dir_str = current_dir.to_string_lossy().to_string();
                
                // Ensure projects section exists
                if config.get("projects").is_none() {
                    config["projects"] = json!({});
                }
                
                // Check if project entry exists
                if config["projects"].get(&current_dir_str).is_none() {
                    return Err(anyhow!(
                        "No Claude project configuration found for this directory. \
                        Please use Claude in this project first, \
                        or use User or Project level registration instead."
                    ));
                }
                
                // Ensure mcpServers exists in project section
                if config["projects"][&current_dir_str].get("mcpServers").is_none() {
                    config["projects"][&current_dir_str]["mcpServers"] = json!({});
                }
                
                config["projects"][&current_dir_str]["mcpServers"][server_name] = server_config.to_json();
            }
            RegistrationLevel::Project => {
                // .mcp.json file
                if config.get("mcpServers").is_none() {
                    config["mcpServers"] = json!({});
                }
                config["mcpServers"][server_name] = server_config.to_json();
            }
        }
        
        self.save_config(level, &config)
    }

    pub fn unregister_server(&self, level: &RegistrationLevel, server_name: &str) -> Result<()> {
        let mut config = self.load_config(level)?;
        
        match level {
            RegistrationLevel::User => {
                // Root-level mcpServers
                if let Some(servers) = config.get_mut("mcpServers") {
                    if let Some(servers_obj) = servers.as_object_mut() {
                        servers_obj.remove(server_name);
                    }
                }
            }
            RegistrationLevel::Local => {
                // Project-specific mcpServers under projects key
                let current_dir = std::env::current_dir()
                    .map_err(|e| anyhow!("Failed to get current directory: {}", e))?;
                let current_dir_str = current_dir.to_string_lossy().to_string();
                
                if let Some(projects) = config.get_mut("projects") {
                    if let Some(project_config) = projects.get_mut(&current_dir_str) {
                        if let Some(servers) = project_config.get_mut("mcpServers") {
                            if let Some(servers_obj) = servers.as_object_mut() {
                                servers_obj.remove(server_name);
                            }
                        }
                    }
                }
            }
            RegistrationLevel::Project => {
                // .mcp.json file
                if let Some(servers) = config.get_mut("mcpServers") {
                    if let Some(servers_obj) = servers.as_object_mut() {
                        servers_obj.remove(server_name);
                    }
                }
            }
        }
        
        self.save_config(level, &config)
    }


}

pub fn get_claude_user_config_path() -> Result<PathBuf> {
    if let Some(home_dir) = dirs::home_dir() {
        Ok(home_dir.join(".claude.json"))
    } else {
        Err(anyhow!("Could not find home directory"))
    }
}

pub fn get_claude_project_config_path() -> Result<PathBuf> {
    let current_dir = std::env::current_dir()
        .map_err(|e| anyhow!("Failed to get current directory: {}", e))?;
    
    Ok(current_dir.join(".mcp.json"))
}


pub fn prompt_projectfiles_registration(manager: &RegistrationManager) -> Result<Vec<(RegistrationLevel, bool)>> {
    // Check if project configuration exists under projects key
    let current_dir = std::env::current_dir()
        .map_err(|e| anyhow!("Failed to get current directory: {}", e))?;
    let current_dir_str = current_dir.to_string_lossy().to_string();
    let config = manager.load_config(&RegistrationLevel::Local)?;
    let has_active_project_config = config.get("projects")
        .and_then(|projects| projects.get(&current_dir_str))
        .is_some();
    
    // Build available levels based on active project configuration
    let mut levels = Vec::new();
    if has_active_project_config {
        levels.push(RegistrationLevel::Local);
    }
    levels.push(RegistrationLevel::User);
    levels.push(RegistrationLevel::Project);
    
    // Check current status for each level
    let mut options = Vec::new();
    let mut default_indices = Vec::new();
    
    for (i, level) in levels.iter().enumerate() {
        let is_registered = manager.is_server_registered(level, "projectfiles")?;
        let description = match level {
            RegistrationLevel::Local => "projectfiles - Local level (project-specific in ~/.claude.json)",
            RegistrationLevel::User => "projectfiles - User level (global in ~/.claude.json)", 
            RegistrationLevel::Project => "projectfiles - Project level (in .mcp.json file)",
        };
        options.push(description.to_string());
        if is_registered {
            default_indices.push(i);
        }
    }
    
    let result = MultiSelect::new("Select MCP server registrations to enable:", options)
        .with_default(&default_indices)
        .prompt();
    
    match result {
        Ok(selected) => {
            let mut changes = Vec::new();
            
            for (_i, level) in levels.iter().enumerate() {
                let option_text = match level {
                    RegistrationLevel::Local => "projectfiles - Local level (project-specific in ~/.claude.json)",
                    RegistrationLevel::User => "projectfiles - User level (global in ~/.claude.json)", 
                    RegistrationLevel::Project => "projectfiles - Project level (in .mcp.json file)",
                };
                let new_state = selected.contains(&option_text.to_string());
                let current_state = manager.is_server_registered(level, "projectfiles")?;
                
                if new_state != current_state {
                    changes.push((level.clone(), new_state));
                }
            }
            
            Ok(changes)
        }
        Err(_) => Ok(Vec::new()), // User cancelled/escaped
    }
}