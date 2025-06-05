use serde_json::Value;
use super::errors::QueryError;

pub struct QueryParser;

#[derive(Debug, Clone)]
pub struct ParsedQuery {
    pub raw: String,
    pub query_type: QueryType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum QueryType {
    Path(String),
    Pipe(Vec<String>),
    Conditional(ConditionalExpr),
    Assignment(String, Value),
    Expression(String),
    ArrayConstruction(String),
    ObjectConstruction(String),
    Function(String, Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConditionalExpr {
    pub condition: String,
    pub then_expr: String,
    pub else_expr: Option<String>,
}

impl QueryParser {
    pub fn new() -> Self {
        Self
    }
    
    /// Parse a value from a string
    pub fn parse_value(&self, value_str: &str) -> Result<Value, QueryError> {
        let value_str = value_str.trim();
        
        // Boolean values
        if value_str == "true" {
            return Ok(Value::Bool(true));
        }
        if value_str == "false" {
            return Ok(Value::Bool(false));
        }
        
        // Null
        if value_str == "null" {
            return Ok(Value::Null);
        }
        
        // Numbers
        if let Ok(num) = value_str.parse::<i64>() {
            return Ok(Value::Number(serde_json::Number::from(num)));
        }
        if let Ok(num) = value_str.parse::<f64>() {
            if let Some(n) = serde_json::Number::from_f64(num) {
                return Ok(Value::Number(n));
            }
        }
        
        // JSON values (strings, arrays, objects)
        if (value_str.starts_with('"') && value_str.ends_with('"')) ||
           (value_str.starts_with('[') && value_str.ends_with(']')) ||
           (value_str.starts_with('{') && value_str.ends_with('}')) {
            return serde_json::from_str(value_str)
                .map_err(|e| QueryError::InvalidSyntax(format!("Invalid JSON: {}", e)));
        }
        
        // Treat as unquoted string
        Ok(Value::String(value_str.to_string()))
    }
    
    /// Parse an assignment expression like ".path = value"
    pub fn parse_assignment(&self, query: &str) -> Result<Option<(String, Value)>, QueryError> {
        if let Some(eq_pos) = query.find(" = ") {
            let path = query[..eq_pos].trim().to_string();
            let value_str = query[eq_pos + 3..].trim();
            let value = self.parse_value(value_str)?;
            Ok(Some((path, value)))
        } else {
            Ok(None)
        }
    }
    
    /// Check if a string is an expression (arithmetic, comparison, etc.)
    pub fn is_expression(&self, query: &str) -> bool {
        // Check for arithmetic operators
        if query.contains(" + ") || query.contains(" - ") || 
           query.contains(" * ") || query.contains(" / ") || 
           query.contains(" % ") {
            return true;
        }
        
        // Check for comparison operators
        if query.contains(" == ") || query.contains(" != ") ||
           query.contains(" < ") || query.contains(" > ") ||
           query.contains(" <= ") || query.contains(" >= ") {
            return true;
        }
        
        // Check for logical operators
        if query.contains(" and ") || query.contains(" or ") {
            return true;
        }
        
        // Check for not operator
        if query.trim().starts_with("not ") {
            return true;
        }
        
        false
    }
    
    /// Parse a pipe expression
    pub fn parse_pipe_expression(&self, query: &str) -> Vec<String> {
        query.split(" | ")
            .map(|s| s.trim().to_string())
            .collect()
    }
    
    /// Parse a conditional expression
    pub fn parse_conditional(&self, query: &str) -> Result<ConditionalExpr, QueryError> {
        // Simple regex-like parsing for if-then-else
        let query = query.trim();
        if !query.starts_with("if ") {
            return Err(QueryError::InvalidSyntax("Conditional must start with 'if'".to_string()));
        }
        
        // Find the first "then" after "if"
        let then_pos = query.find(" then ")
            .ok_or_else(|| QueryError::InvalidSyntax("Missing 'then' in conditional".to_string()))?;
        
        let condition = query[3..then_pos].trim().to_string();
        
        // Now we need to find the matching "else" and "end" for this "if"
        // Count nested if/end pairs to find the correct positions
        let mut depth = 0;
        let mut else_pos = None;
        let mut end_pos = None;
        
        let chars: Vec<char> = query.chars().collect();
        let mut i = then_pos + 6; // Start after " then "
        
        while i < chars.len() {
            // Check for keywords
            if i + 3 <= chars.len() && &chars[i..i+3] == ['i', 'f', ' '] {
                // Check if it's actually "if " and not part of another word
                if i == 0 || !chars[i-1].is_alphanumeric() {
                    depth += 1;
                    i += 3;
                    continue;
                }
            }
            
            if i + 3 <= chars.len() && &chars[i..i+3] == ['e', 'n', 'd'] {
                // Check if it's actually "end" and not part of another word
                let is_word_boundary_before = i == 0 || !chars[i-1].is_alphanumeric();
                let is_word_boundary_after = i + 3 == chars.len() || (i + 3 < chars.len() && !chars[i+3].is_alphanumeric());
                
                if is_word_boundary_before && is_word_boundary_after {
                    if depth == 0 {
                        end_pos = Some(i);
                        break;
                    } else {
                        depth -= 1;
                    }
                    i += 3;
                    continue;
                }
            }
            
            if depth == 0 && else_pos.is_none() && i + 5 <= chars.len() && &chars[i..i+5] == ['e', 'l', 's', 'e', ' '] {
                // Check if it's actually "else " and not part of another word
                let is_word_boundary = i == 0 || !chars[i-1].is_alphanumeric();
                if is_word_boundary {
                    else_pos = Some(i);
                    i += 5;
                    continue;
                }
            }
            
            i += 1;
        }
        
        let end_pos = end_pos
            .ok_or_else(|| QueryError::InvalidSyntax("Missing 'end' in conditional".to_string()))?;
        
        let then_expr = if let Some(else_pos) = else_pos {
            query[then_pos + 6..else_pos].trim().to_string()
        } else {
            query[then_pos + 6..end_pos].trim().to_string()
        };
        
        let else_expr = else_pos.map(|pos| query[pos + 5..end_pos].trim().to_string());
        
        Ok(ConditionalExpr {
            condition,
            then_expr,
            else_expr,
        })
    }
    
    /// Extract function name and arguments
    pub fn parse_function_call(&self, query: &str) -> Option<(String, String)> {
        if let Some(open_paren) = query.find('(') {
            if query.ends_with(')') {
                let func_name = query[..open_paren].trim();
                
                // Check if this looks like a valid function name (not containing operators)
                if func_name.contains(" + ") || func_name.contains(" - ") ||
                   func_name.contains(" * ") || func_name.contains(" / ") ||
                   func_name.contains(" % ") || func_name.contains(" == ") ||
                   func_name.contains(" != ") || func_name.contains(" < ") ||
                   func_name.contains(" > ") || func_name.contains(" <= ") ||
                   func_name.contains(" >= ") || func_name.contains(" and ") ||
                   func_name.contains(" or ") {
                    return None;
                }
                
                // Also skip if it starts with a dot (path query)
                if func_name.starts_with('.') {
                    return None;
                }
                
                // Don't return empty function names
                if func_name.is_empty() {
                    return None;
                }
                
                let args = query[open_paren + 1..query.len() - 1].trim().to_string();
                return Some((func_name.to_string(), args));
            }
        }
        None
    }
    
    /// Parse path with array/object access
    pub fn parse_complex_path(&self, path: &str) -> Result<Vec<PathSegment>, QueryError> {
        let mut segments = Vec::new();
        let mut current = String::new();
        let mut chars = path.chars().peekable();
        
        while let Some(ch) = chars.next() {
            match ch {
                '.' => {
                    if !current.is_empty() {
                        segments.push(PathSegment::Field(current.clone()));
                        current.clear();
                    }
                }
                '[' => {
                    if !current.is_empty() {
                        segments.push(PathSegment::Field(current.clone()));
                        current.clear();
                    }
                    
                    // Parse array index or slice
                    let mut index_str = String::new();
                    let mut found_close = false;
                    
                    for ch in chars.by_ref() {
                        if ch == ']' {
                            found_close = true;
                            break;
                        }
                        index_str.push(ch);
                    }
                    
                    if !found_close {
                        return Err(QueryError::InvalidSyntax("Unclosed bracket".to_string()));
                    }
                    
                    // Parse the index content
                    if index_str == "*" {
                        segments.push(PathSegment::ArrayAll);
                    } else if index_str.contains(':') {
                        // Array slice
                        let parts: Vec<&str> = index_str.split(':').collect();
                        if parts.len() == 2 {
                            let start = if parts[0].is_empty() {
                                None
                            } else {
                                Some(parts[0].parse().map_err(|_| 
                                    QueryError::InvalidSyntax(format!("Invalid slice start: {}", parts[0]))
                                )?)
                            };
                            let end = if parts[1].is_empty() {
                                None
                            } else {
                                Some(parts[1].parse().map_err(|_| 
                                    QueryError::InvalidSyntax(format!("Invalid slice end: {}", parts[1]))
                                )?)
                            };
                            segments.push(PathSegment::ArraySlice(start, end));
                        } else {
                            return Err(QueryError::InvalidSyntax("Invalid array slice".to_string()));
                        }
                    } else {
                        // Simple index
                        let index = index_str.parse()
                            .map_err(|_| QueryError::InvalidSyntax(format!("Invalid array index: {}", index_str)))?;
                        segments.push(PathSegment::ArrayIndex(index));
                    }
                }
                _ => {
                    current.push(ch);
                }
            }
        }
        
        if !current.is_empty() {
            segments.push(PathSegment::Field(current));
        }
        
        Ok(segments)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PathSegment {
    Field(String),
    ArrayIndex(usize),
    ArrayAll,
    ArraySlice(Option<usize>, Option<usize>),
}

impl Default for QueryParser {
    fn default() -> Self {
        Self::new()
    }
}