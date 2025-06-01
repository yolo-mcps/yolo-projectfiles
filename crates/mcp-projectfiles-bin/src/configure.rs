use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table},
    Frame, Terminal,
};
use std::io::{self, Stdout};

use crate::{
    permissions::{PermissionsManager, PermissionLevel},
    registration::{RegistrationManager, McpServerConfig, CLAUDE, RegistrationLevel},
};

#[derive(Debug, Clone, PartialEq)]
enum Section {
    Registration,
    Permissions,
}

pub struct ConfigureApp {
    active_section: Section,
    registration_manager: RegistrationManager,
    permissions_manager: PermissionsManager,
    
    // Registration state
    register_selections: Vec<(RegistrationLevel, bool, bool)>, // (level, is_registered, should_register)
    register_selected_index: usize,
    
    // Permissions state
    permission_configs: Vec<(PermissionLevel, Vec<(String, bool)>)>, // (level, [(tool, enabled)])
    permissions_selected_row: usize,  // Which tool (row)
    permissions_selected_col: usize,  // Which level (column)
    
    // General state
    show_help: bool,
    has_unsaved_changes: bool,
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl ConfigureApp {
    pub fn new() -> Result<Self> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        
        let registration_manager = RegistrationManager::new(CLAUDE);
        let permissions_manager = PermissionsManager::new("projectfiles".to_string());
        
        // Initialize registration state
        let mut register_selections = Vec::new();
        for level in [RegistrationLevel::User, RegistrationLevel::Project, RegistrationLevel::Local] {
            if let Ok(is_registered) = registration_manager.is_server_registered(&level, "projectfiles") {
                register_selections.push((level, is_registered, is_registered));
            }
        }
        
        // Initialize permissions state
        let available_tools = permissions_manager.get_available_tools();
        let mut permission_configs = Vec::new();
        
        for level in [PermissionLevel::User, PermissionLevel::Project, PermissionLevel::Local] {
            let (allowed, _) = permissions_manager.get_tool_permissions(&level)?;
            let tool_states: Vec<(String, bool)> = available_tools.iter()
                .map(|tool| {
                    // Check for MCP-prefixed name
                    let mcp_tool_name = format!("mcp__projectfiles__{}", tool);
                    let is_allowed = allowed.contains(&mcp_tool_name);
                    (tool.clone(), is_allowed)
                })
                .collect();
            permission_configs.push((level, tool_states));
        }
        
        Ok(Self {
            active_section: Section::Registration,
            registration_manager,
            permissions_manager,
            register_selections,
            register_selected_index: 0,
            permission_configs,
            permissions_selected_row: 0,
            permissions_selected_col: 0,
            show_help: false,
            has_unsaved_changes: false,
            terminal,
        })
    }
    
    pub fn run(mut self) -> Result<()> {
        loop {
            self.draw_ui()?;
            
            if let Event::Key(key) = event::read()? {
                if self.handle_input(key)? {
                    break;
                }
            }
        }
        
        self.cleanup()?;
        Ok(())
    }
    
    fn draw_ui(&mut self) -> Result<()> {
        let active_section = self.active_section.clone();
        let register_selections = self.register_selections.clone();
        let register_selected_index = self.register_selected_index;
        let permission_configs = self.permission_configs.clone();
        let permissions_selected_row = self.permissions_selected_row;
        let permissions_selected_col = self.permissions_selected_col;
        let show_help = self.show_help;
        let has_unsaved_changes = self.has_unsaved_changes;
        
        self.terminal.draw(|f| {
            Self::render_ui(
                f,
                active_section,
                &register_selections,
                register_selected_index,
                &permission_configs,
                permissions_selected_row,
                permissions_selected_col,
                show_help,
                has_unsaved_changes,
            )
        })?;
        Ok(())
    }
    
    fn cleanup(&mut self) -> Result<()> {
        disable_raw_mode()?;
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)?;
        self.terminal.show_cursor()?;
        Ok(())
    }
    
    fn handle_input(&mut self, key: KeyEvent) -> Result<bool> {
        // Global keys
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc if !self.show_help => {
                if self.has_unsaved_changes {
                    // TODO: Add confirmation dialog
                }
                return Ok(true);
            }
            KeyCode::Char('?') => {
                self.show_help = !self.show_help;
                return Ok(false);
            }
            KeyCode::Esc if self.show_help => {
                self.show_help = false;
                return Ok(false);
            }
            _ if self.show_help => return Ok(false),
            _ => {}
        }
        
        // Section navigation
        match key.code {
            KeyCode::Tab => {
                self.active_section = match self.active_section {
                    Section::Registration => Section::Permissions,
                    Section::Permissions => Section::Registration,
                };
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.save_all_changes()?;
            }
            _ => {
                // Section-specific input handling
                match self.active_section {
                    Section::Registration => self.handle_register_input(key)?,
                    Section::Permissions => self.handle_permissions_input(key)?,
                }
            }
        }
        
        Ok(false)
    }
    
    fn handle_register_input(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.register_selected_index > 0 {
                    self.register_selected_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.register_selected_index < self.register_selections.len() - 1 {
                    self.register_selected_index += 1;
                }
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                if let Some((_, is_registered, should_register)) = self.register_selections.get_mut(self.register_selected_index) {
                    *should_register = !*should_register;
                    self.has_unsaved_changes = *is_registered != *should_register;
                }
            }
            _ => {}
        }
        Ok(())
    }
    
    fn handle_permissions_input(&mut self, key: KeyEvent) -> Result<()> {
        // Get the number of tools (rows)
        let num_tools = if let Some((_, tools)) = self.permission_configs.first() {
            tools.len()
        } else {
            0
        };
        
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.permissions_selected_row > 0 {
                    self.permissions_selected_row -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.permissions_selected_row < num_tools - 1 {
                    self.permissions_selected_row += 1;
                }
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if self.permissions_selected_col > 0 {
                    self.permissions_selected_col -= 1;
                }
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if self.permissions_selected_col < 2 { // 3 columns: 0, 1, 2
                    self.permissions_selected_col += 1;
                }
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                // Toggle the selected permission
                if let Some((_, tools)) = self.permission_configs.get_mut(self.permissions_selected_col) {
                    if let Some((_, enabled)) = tools.get_mut(self.permissions_selected_row) {
                        *enabled = !*enabled;
                        self.has_unsaved_changes = true;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    
    fn save_all_changes(&mut self) -> Result<()> {
        // Save registration changes
        let config = McpServerConfig::new_stdio();
        for (level, is_registered, should_register) in &mut self.register_selections {
            if is_registered != should_register {
                if *should_register {
                    self.registration_manager.register_server(level, "projectfiles", &config)?;
                } else {
                    self.registration_manager.unregister_server(level, "projectfiles")?;
                }
                *is_registered = *should_register;
            }
        }
        
        // Save permission changes
        for (level, tools) in &self.permission_configs {
            let allowed_tools: Vec<String> = tools.iter()
                .filter_map(|(tool, enabled)| if *enabled { Some(tool.clone()) } else { None })
                .collect();
            self.permissions_manager.update_tool_permissions(level, allowed_tools)?;
        }
        
        self.has_unsaved_changes = false;
        Ok(())
    }
    
    fn render_ui(
        f: &mut Frame,
        active_section: Section,
        register_selections: &[(RegistrationLevel, bool, bool)],
        register_selected_index: usize,
        permission_configs: &[(PermissionLevel, Vec<(String, bool)>)],
        permissions_selected_row: usize,
        permissions_selected_col: usize,
        show_help: bool,
        has_unsaved_changes: bool,
    ) {
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),     // Main content
                Constraint::Length(3),  // Status bar
            ])
            .split(f.area());
        
        // Split main area into two sections
        let content_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(40),  // Registration
                Constraint::Percentage(60),  // Permissions
            ])
            .split(main_chunks[0]);
        
        // Render registration section
        Self::render_registration_section(
            f,
            content_chunks[0],
            active_section == Section::Registration,
            register_selections,
            register_selected_index,
        );
        
        // Render permissions section
        Self::render_permissions_section(
            f,
            content_chunks[1],
            active_section == Section::Permissions,
            permission_configs,
            permissions_selected_row,
            permissions_selected_col,
        );
        
        // Render status bar
        Self::render_status_bar(f, main_chunks[1], has_unsaved_changes);
        
        // Render help overlay if active
        if show_help {
            Self::render_help_overlay(f);
        }
    }
    
    fn render_registration_section(
        f: &mut Frame,
        area: Rect,
        active: bool,
        register_selections: &[(RegistrationLevel, bool, bool)],
        register_selected_index: usize,
    ) {
        let border_color = if active { Color::Yellow } else { Color::White };
        
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(format!("Registration{}", if active { " (Active)" } else { "" }));
        
        let items: Vec<ListItem> = register_selections.iter()
            .map(|(level, is_registered, should_register)| {
                let checkbox = if *should_register { "[✓]" } else { "[ ]" };
                let modified = is_registered != should_register;
                
                let style = if modified {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::ITALIC)
                } else if *is_registered {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                };
                
                ListItem::new(format!("  {} {:?}", checkbox, level)).style(style)
            })
            .collect();
        
        let list = List::new(items)
            .block(block)
            .highlight_style(if active {
                Style::default().add_modifier(Modifier::BOLD).bg(Color::DarkGray)
            } else {
                Style::default().fg(Color::DarkGray)
            })
            .highlight_symbol("> ");
        
        let mut state = ListState::default();
        state.select(Some(register_selected_index));
        f.render_stateful_widget(list, area, &mut state);
    }
    
    fn render_permissions_section(
        f: &mut Frame,
        area: Rect,
        active: bool,
        permission_configs: &[(PermissionLevel, Vec<(String, bool)>)],
        selected_row: usize,
        selected_col: usize,
    ) {
        let border_color = if active { Color::Yellow } else { Color::White };
        
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(format!("Tool Permissions{}", if active { " (Active)" } else { "" }));
        
        // Build header row
        let header_cells = vec![
            Cell::from("Tool").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("User").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Project").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Local").style(Style::default().add_modifier(Modifier::BOLD)),
        ];
        let header = Row::new(header_cells)
            .style(Style::default().fg(Color::Cyan))
            .height(1);
        
        // Build data rows
        let mut rows = Vec::new();
        
        // Get tool names from the first level's tools
        if let Some((_, tools)) = permission_configs.first() {
            for (i, (tool, _)) in tools.iter().enumerate() {
                let parts: Vec<&str> = tool.split("__").collect();
                let tool_name = if parts.len() >= 3 { parts[2] } else { tool };
                
                let mut cells = vec![Cell::from(tool_name)];
                
                // Add cells for each permission level (User, Project, Local)
                for (col, (_, level_tools)) in permission_configs.iter().enumerate() {
                    if let Some((_, enabled)) = level_tools.get(i) {
                        let symbol = if *enabled { "✓" } else { "-" };
                        let style = if active && i == selected_row && col == selected_col {
                            Style::default()
                                .fg(if *enabled { Color::Green } else { Color::DarkGray })
                                .add_modifier(Modifier::BOLD)
                                .bg(Color::DarkGray)
                        } else {
                            Style::default()
                                .fg(if *enabled { Color::Green } else { Color::DarkGray })
                        };
                        cells.push(Cell::from(symbol).style(style));
                    }
                }
                
                rows.push(Row::new(cells).height(1));
            }
        }
        
        // Create table with appropriate column widths
        let table = Table::new(rows, [
            Constraint::Length(10),  // Tool name
            Constraint::Length(8),   // User
            Constraint::Length(8),   // Project
            Constraint::Length(8),   // Local
        ])
        .header(header)
        .block(block)
        .column_spacing(1);
        
        f.render_widget(table, area);
    }
    
    fn render_status_bar(f: &mut Frame, area: Rect, has_unsaved_changes: bool) {
        let status = if has_unsaved_changes {
            " [Modified]"
        } else {
            ""
        };
        
        let help_text = format!(
            "Tab: Switch sections | ↑↓←→/hjkl: Navigate | Space: Toggle | Ctrl+S: Save{} | ?: Help | q: Quit",
            status
        );
        
        let style = if has_unsaved_changes {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        
        let paragraph = Paragraph::new(help_text)
            .block(Block::default().borders(Borders::ALL))
            .style(style);
        
        f.render_widget(paragraph, area);
    }
    
    fn render_help_overlay(f: &mut Frame) {
        let area = centered_rect(60, 60, f.area());
        f.render_widget(Clear, area);
        
        let help_text = vec![
            Line::from(vec![Span::styled("Claude Configure Help", Style::default().add_modifier(Modifier::BOLD).fg(Color::Cyan))]),
            Line::from(""),
            Line::from(vec![Span::styled("Navigation", Style::default().add_modifier(Modifier::BOLD))]),
            Line::from("  Tab              Switch between Registration and Permissions"),
            Line::from("  ↑↓/jk            Move selection up/down (Registration) or between tools (Permissions)"),
            Line::from("  ←→/hl            Move between permission levels (Permissions only)"),
            Line::from("  Space/Enter      Toggle checkbox"),
            Line::from(""),
            Line::from(vec![Span::styled("Actions", Style::default().add_modifier(Modifier::BOLD))]),
            Line::from("  Ctrl+S           Save all changes"),
            Line::from("  ?                Toggle this help"),
            Line::from("  q/Esc            Quit"),
            Line::from(""),
            Line::from(vec![Span::styled("Registration Levels", Style::default().add_modifier(Modifier::BOLD))]),
            Line::from("  User             Global for current user"),
            Line::from("  Project          Checked into repository (.mcp.json)"),
            Line::from("  Local            Project-specific, not checked in"),
            Line::from(""),
            Line::from(vec![Span::styled("Permission Levels", Style::default().add_modifier(Modifier::BOLD))]),
            Line::from("  User             ~/.claude/settings.json"),
            Line::from("  Project          .claude/settings.json"),
            Line::from("  Local            .claude/settings.local.json"),
            Line::from(""),
            Line::from(vec![Span::styled("Visual Indicators", Style::default().add_modifier(Modifier::BOLD))]),
            Line::from("  Yellow text      Modified (unsaved changes)"),
            Line::from("  Green text       Enabled/Registered"),
            Line::from("  Yellow border    Active section"),
            Line::from(""),
            Line::from(vec![Span::styled("Permission States", Style::default().add_modifier(Modifier::BOLD))]),
            Line::from("  ✓                Tool explicitly allowed (no permission prompt)"),
            Line::from("  -                Tool not specified (requires permission prompt)"),
        ];
        
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Help")
            .style(Style::default().bg(Color::Black));
        
        let paragraph = Paragraph::new(help_text)
            .block(block);
        
        f.render_widget(paragraph, area);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

pub fn run_configure() -> Result<()> {
    let app = ConfigureApp::new()?;
    app.run()
}