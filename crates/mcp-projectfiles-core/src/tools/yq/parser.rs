use super::YamlQueryError;
use serde_json;

/// Parse assignment expressions like ".field = value"
#[allow(dead_code)]
pub fn parse_assignment(query: &str) -> Result<Option<(String, serde_json::Value)>, YamlQueryError> {
    // Parse simple assignment patterns like ".field = value"
    if let Some(eq_pos) = query.find('=') {
        let path = query[..eq_pos].trim();
        let value_str = query[eq_pos + 1..].trim();
        
        // Parse the value as JSON, handling different types properly
        let value = parse_value(value_str)?;
        
        Ok(Some((path.to_string(), value)))
    } else {
        Ok(None)
    }
}

/// Parse a value string into a JSON value
pub fn parse_value(value_str: &str) -> Result<serde_json::Value, YamlQueryError> {
    let value_str = value_str.trim();
    
    if value_str == "true" {
        Ok(serde_json::Value::Bool(true))
    } else if value_str == "false" {
        Ok(serde_json::Value::Bool(false))
    } else if value_str == "null" {
        Ok(serde_json::Value::Null)
    } else if let Ok(num) = value_str.parse::<i64>() {
        Ok(serde_json::Value::Number(serde_json::Number::from(num)))
    } else if let Ok(num) = value_str.parse::<f64>() {
        Ok(serde_json::Value::Number(serde_json::Number::from_f64(num).unwrap_or(serde_json::Number::from(0))))
    } else if value_str.starts_with('"') && value_str.ends_with('"') {
        // Already quoted string - parse as JSON
        serde_json::from_str(value_str)
            .map_err(|e| YamlQueryError::InvalidQuery(format!("Invalid JSON string '{}': {}", value_str, e)))
    } else if value_str.starts_with('[') || value_str.starts_with('{') {
        // JSON array or object
        serde_json::from_str(value_str)
            .map_err(|e| YamlQueryError::InvalidQuery(format!("Invalid JSON '{}': {}", value_str, e)))
    } else {
        // Treat as unquoted string
        Ok(serde_json::Value::String(value_str.to_string()))
    }
}

/// Parse a string argument from a function call (e.g., the "," in split(","))
pub fn parse_string_arg(arg: &str) -> Result<String, YamlQueryError> {
    let arg = arg.trim();
    if arg.starts_with('"') && arg.ends_with('"') && arg.len() >= 2 {
        Ok(arg[1..arg.len()-1].to_string())
    } else {
        Err(YamlQueryError::InvalidQuery(format!("String argument must be quoted: {}", arg)))
    }
}

/// Check if a query contains a pipe operator outside of parentheses
pub fn contains_pipe_outside_parens(query: &str) -> bool {
    let mut paren_depth: i32 = 0;
    let mut in_string = false;
    let mut escape_next = false;
    let chars: Vec<char> = query.chars().collect();
    
    for i in 0..chars.len() {
        if escape_next {
            escape_next = false;
            continue;
        }
        
        match chars[i] {
            '\\' if in_string => escape_next = true,
            '"' => in_string = !in_string,
            '(' if !in_string => paren_depth += 1,
            ')' if !in_string => paren_depth = paren_depth.saturating_sub(1),
            '|' if !in_string && paren_depth == 0 => return true,
            _ => {}
        }
    }
    
    false
}

/// Parse pipe expression into separate parts
pub fn parse_pipe_expression(query: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current_part = String::new();
    let mut paren_depth: i32 = 0;
    let mut in_string = false;
    let mut escape_next = false;
    let chars: Vec<char> = query.chars().collect();
    
    for i in 0..chars.len() {
        if escape_next {
            escape_next = false;
            current_part.push(chars[i]);
            continue;
        }
        
        match chars[i] {
            '\\' if in_string => {
                escape_next = true;
                current_part.push(chars[i]);
            },
            '"' => {
                in_string = !in_string;
                current_part.push(chars[i]);
            },
            '(' if !in_string => {
                paren_depth += 1;
                current_part.push(chars[i]);
            },
            ')' if !in_string => {
                paren_depth = paren_depth.saturating_sub(1);
                current_part.push(chars[i]);
            },
            '|' if !in_string && paren_depth == 0 => {
                parts.push(current_part.trim().to_string());
                current_part.clear();
            },
            _ => {
                current_part.push(chars[i]);
            }
        }
    }
    
    if !current_part.trim().is_empty() {
        parts.push(current_part.trim().to_string());
    }
    
    parts
}

/// Check if an expression is an arithmetic operation
pub fn is_arithmetic_expression(query: &str) -> bool {
    // Skip if it starts with a dot (path query)
    if query.starts_with('.') && !query.contains(" ") {
        return false;
    }
    
    // Skip if it's object construction
    if query.starts_with('{') && query.ends_with('}') {
        return false;
    }
    
    // Skip if it's array construction
    if query.starts_with('[') && query.ends_with(']') {
        return false;
    }
    
    // Check for arithmetic operators
    query.contains(" + ") || query.contains(" - ") || 
    query.contains(" * ") || query.contains(" / ") ||
    query.contains(" % ") ||
    query.starts_with('(') && query.ends_with(')')
}

/// Check if an expression is a comparison operation
pub fn is_comparison_expression(query: &str) -> bool {
    // Skip if it's object construction
    if query.starts_with('{') && query.ends_with('}') {
        return false;
    }
    
    // Skip if it's array construction
    if query.starts_with('[') && query.ends_with(']') {
        return false;
    }
    
    // Check for comparison operators
    query.contains(" == ") || query.contains(" != ") || 
    query.contains(" > ") || query.contains(" < ") ||
    query.contains(" >= ") || query.contains(" <= ")
}

/// Check if an expression is a logical operation
pub fn is_logical_expression(query: &str) -> bool {
    // Skip if it's object construction
    if query.starts_with('{') && query.ends_with('}') {
        return false;
    }
    
    // Skip if it's array construction
    if query.starts_with('[') && query.ends_with(']') {
        return false;
    }
    
    // Check for logical operators
    query.contains(" and ") || query.contains(" or ") || 
    query.starts_with("not ")
}

/// Extract function name and arguments from a function call like "func(arg1, arg2)"
#[allow(dead_code)]
pub fn parse_function_call(query: &str) -> Result<(String, Vec<String>), YamlQueryError> {
    if let Some(paren_pos) = query.find('(') {
        if !query.ends_with(')') {
            return Err(YamlQueryError::InvalidQuery("Function call missing closing parenthesis".to_string()));
        }
        
        let func_name = query[..paren_pos].trim().to_string();
        let args_str = &query[paren_pos + 1..query.len() - 1];
        
        // Simple argument parsing (doesn't handle nested parentheses perfectly)
        let args: Vec<String> = if args_str.trim().is_empty() {
            Vec::new()
        } else {
            args_str.split(',').map(|s| s.trim().to_string()).collect()
        };
        
        Ok((func_name, args))
    } else {
        Err(YamlQueryError::InvalidQuery("Not a function call".to_string()))
    }
}

/// Parse object key-value pairs from object construction syntax
#[allow(dead_code)]
pub fn parse_object_construction(content: &str) -> Result<Vec<(String, String)>, YamlQueryError> {
    let mut pairs = Vec::new();
    let parts: Vec<&str> = content.split(',').map(|s| s.trim()).collect();
    
    for part in parts {
        if part.is_empty() {
            continue;
        }
        
        if let Some(colon_pos) = part.find(':') {
            let key_part = part[..colon_pos].trim().to_string();
            let value_part = part[colon_pos + 1..].trim().to_string();
            pairs.push((key_part, value_part));
        } else {
            return Err(YamlQueryError::InvalidQuery("Invalid object construction syntax".to_string()));
        }
    }
    
    Ok(pairs)
}