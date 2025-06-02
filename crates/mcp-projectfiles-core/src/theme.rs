use colored::*;
use std::env;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DiffTheme {
    GitHub,
    GitLab,
    Monokai,
    Solarized,
    Dracula,
    Classic,
    None,
}

impl DiffTheme {
    /// Parse theme from environment variable value
    pub fn from_env_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "github" => DiffTheme::GitHub,
            "gitlab" => DiffTheme::GitLab,
            "monokai" => DiffTheme::Monokai,
            "solarized" => DiffTheme::Solarized,
            "dracula" => DiffTheme::Dracula,
            "classic" => DiffTheme::Classic,
            "none" | "no-color" | "nocolor" => DiffTheme::None,
            _ => DiffTheme::GitHub, // Default
        }
    }
    
    /// Get the current theme from environment
    pub fn current() -> Self {
        // Check NO_COLOR first (standard environment variable)
        if env::var("NO_COLOR").is_ok() {
            return DiffTheme::None;
        }
        
        // Then check our custom theme variable
        match env::var("YOLO_PROJECTFILES_THEME") {
            Ok(theme) => Self::from_env_str(&theme),
            Err(_) => DiffTheme::GitHub, // Default
        }
    }
    
    /// Apply theme colors to diff components
    pub fn colorize_header_old(&self, text: &str) -> String {
        match self {
            DiffTheme::None => text.to_string(),
            DiffTheme::GitHub => text.truecolor(215, 58, 73).to_string(), // GitHub red
            DiffTheme::GitLab => text.truecolor(251, 152, 155).to_string(), // GitLab red
            DiffTheme::Monokai => text.truecolor(249, 38, 114).to_string(), // Monokai pink
            DiffTheme::Solarized => text.truecolor(220, 50, 47).to_string(), // Solarized red
            DiffTheme::Dracula => text.truecolor(255, 85, 85).to_string(), // Dracula red
            DiffTheme::Classic => text.red().to_string(),
        }
    }
    
    pub fn colorize_header_new(&self, text: &str) -> String {
        match self {
            DiffTheme::None => text.to_string(),
            DiffTheme::GitHub => text.truecolor(87, 171, 90).to_string(), // GitHub green
            DiffTheme::GitLab => text.truecolor(74, 179, 126).to_string(), // GitLab green
            DiffTheme::Monokai => text.truecolor(166, 226, 46).to_string(), // Monokai green
            DiffTheme::Solarized => text.truecolor(133, 153, 0).to_string(), // Solarized green
            DiffTheme::Dracula => text.truecolor(80, 250, 123).to_string(), // Dracula green
            DiffTheme::Classic => text.green().to_string(),
        }
    }
    
    pub fn colorize_hunk_header(&self, text: &str) -> String {
        match self {
            DiffTheme::None => text.to_string(),
            DiffTheme::GitHub => text.truecolor(106, 115, 125).to_string(), // GitHub gray
            DiffTheme::GitLab => text.truecolor(31, 117, 203).to_string(), // GitLab blue
            DiffTheme::Monokai => text.truecolor(230, 219, 116).to_string(), // Monokai yellow
            DiffTheme::Solarized => text.truecolor(38, 139, 210).to_string(), // Solarized blue
            DiffTheme::Dracula => text.truecolor(189, 147, 249).to_string(), // Dracula purple
            DiffTheme::Classic => text.cyan().to_string(),
        }
    }
    
    pub fn colorize_deletion(&self, text: &str) -> String {
        match self {
            DiffTheme::None => text.to_string(),
            DiffTheme::GitHub => text.on_truecolor(255, 238, 240).truecolor(215, 58, 73).to_string(),
            DiffTheme::GitLab => text.on_truecolor(251, 229, 225).truecolor(251, 152, 155).to_string(),
            DiffTheme::Monokai => text.truecolor(249, 38, 114).to_string(),
            DiffTheme::Solarized => text.truecolor(220, 50, 47).to_string(),
            DiffTheme::Dracula => text.truecolor(255, 85, 85).to_string(),
            DiffTheme::Classic => text.red().to_string(),
        }
    }
    
    pub fn colorize_addition(&self, text: &str) -> String {
        match self {
            DiffTheme::None => text.to_string(),
            DiffTheme::GitHub => text.on_truecolor(230, 255, 237).truecolor(87, 171, 90).to_string(),
            DiffTheme::GitLab => text.on_truecolor(236, 253, 240).truecolor(74, 179, 126).to_string(),
            DiffTheme::Monokai => text.truecolor(166, 226, 46).to_string(),
            DiffTheme::Solarized => text.truecolor(133, 153, 0).to_string(),
            DiffTheme::Dracula => text.truecolor(80, 250, 123).to_string(),
            DiffTheme::Classic => text.green().to_string(),
        }
    }
    
    pub fn colorize_deletion_marker(&self, text: &str) -> String {
        match self {
            DiffTheme::None => text.to_string(),
            DiffTheme::GitHub => text.on_truecolor(255, 238, 240).truecolor(215, 58, 73).to_string(),
            DiffTheme::GitLab => text.on_truecolor(251, 229, 225).truecolor(251, 152, 155).to_string(),
            DiffTheme::Monokai => text.truecolor(249, 38, 114).to_string(),
            DiffTheme::Solarized => text.truecolor(220, 50, 47).to_string(),
            DiffTheme::Dracula => text.truecolor(255, 85, 85).to_string(),
            DiffTheme::Classic => text.red().to_string(),
        }
    }
    
    pub fn colorize_addition_marker(&self, text: &str) -> String {
        match self {
            DiffTheme::None => text.to_string(),
            DiffTheme::GitHub => text.on_truecolor(230, 255, 237).truecolor(87, 171, 90).to_string(),
            DiffTheme::GitLab => text.on_truecolor(236, 253, 240).truecolor(74, 179, 126).to_string(),
            DiffTheme::Monokai => text.truecolor(166, 226, 46).to_string(),
            DiffTheme::Solarized => text.truecolor(133, 153, 0).to_string(),
            DiffTheme::Dracula => text.truecolor(80, 250, 123).to_string(),
            DiffTheme::Classic => text.green().to_string(),
        }
    }
}