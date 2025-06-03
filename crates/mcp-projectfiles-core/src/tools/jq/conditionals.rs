use super::{JsonQueryError, JsonQueryExecutor, functions};
use serde_json;

fn find_matching_end(query: &str) -> Result<usize, JsonQueryError> {
    let mut if_count = 1; // Start with 1 since we know we're in an if statement
    let mut i = 3; // Skip the initial "if " 
    let chars: Vec<char> = query.chars().collect();
    
    while i < chars.len() {
        // Look for "if " (with space before or at start)
        if i + 2 < chars.len() && 
           chars[i] == 'i' && chars[i+1] == 'f' && chars[i+2] == ' ' &&
           (i == 0 || chars[i-1] == ' ') {
            if_count += 1;
            i += 3;
        }
        // Look for " end" or "end" at end of string
        else if i + 2 < chars.len() && 
                chars[i] == 'e' && chars[i+1] == 'n' && chars[i+2] == 'd' &&
                (i == 0 || chars[i-1] == ' ') &&
                (i + 3 == chars.len() || chars[i+3] == ' ' || chars[i+3] == ')') {
            if_count -= 1;
            if if_count == 0 {
                return Ok(i + 3);
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    
    Err(JsonQueryError::InvalidQuery("Missing 'end' in conditional".to_string()))
}

fn find_else_in_block(block: &str) -> Option<usize> {
    let mut if_count = 1; // We start inside the first if
    let mut i = 3; // Skip the initial "if " since we already know about it
    let chars: Vec<char> = block.chars().collect();
    
    while i < chars.len() {
        // Look for "if " (with space before or at start)
        if i + 2 < chars.len() && 
           chars[i] == 'i' && chars[i+1] == 'f' && chars[i+2] == ' ' &&
           (i == 0 || chars[i-1] == ' ') {
            if_count += 1;
            i += 3;
        }
        // Look for "end" (with space before or at start)
        else if i + 2 < chars.len() && 
                chars[i] == 'e' && chars[i+1] == 'n' && chars[i+2] == 'd' &&
                (i == 0 || chars[i-1] == ' ') &&
                (i + 3 == chars.len() || chars[i+3] == ' ') {
            if_count -= 1;
            i += 3;
        }
        // Look for "else " at the same nesting level
        else if if_count == 1 && i + 4 < chars.len() && 
                chars[i] == 'e' && chars[i+1] == 'l' && 
                chars[i+2] == 's' && chars[i+3] == 'e' && chars[i+4] == ' ' {
            return Some(i);
        } else {
            i += 1;
        }
    }
    
    None
}

fn execute_expression(executor: &JsonQueryExecutor, data: &serde_json::Value, expr: &str) -> Result<serde_json::Value, JsonQueryError> {
    let expr = expr.trim();
    
    // Check if it's an object construction (before trying to parse as JSON)
    if expr.starts_with('{') && expr.ends_with('}') {
        return executor.execute_query(data, expr);
    }
    
    // Check if it's a quoted string literal
    if expr.starts_with('"') && expr.ends_with('"') && expr.len() >= 2 {
        // Return the string without quotes
        return Ok(serde_json::Value::String(expr[1..expr.len()-1].to_string()));
    }
    
    // Check if it's a simple literal value (true, false, null, number)
    if expr == "true" {
        return Ok(serde_json::Value::Bool(true));
    } else if expr == "false" {
        return Ok(serde_json::Value::Bool(false));
    } else if expr == "null" {
        return Ok(serde_json::Value::Null);
    } else if let Ok(num) = expr.parse::<i64>() {
        return Ok(serde_json::Value::Number(serde_json::Number::from(num)));
    } else if let Ok(num) = expr.parse::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(num) {
            return Ok(serde_json::Value::Number(n));
        }
    }
    
    // Otherwise, execute as a query expression
    executor.execute_query(data, expr)
}

pub fn execute_if_then_else(executor: &JsonQueryExecutor, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, JsonQueryError> {
    // Parse if-then-else structure: if CONDITION then EXPR else EXPR end
    // Also support: if CONDITION then EXPR end (without else)
    
    if !query.starts_with("if ") {
        return Err(JsonQueryError::InvalidQuery("Conditional must start with 'if'".to_string()));
    }
    
    // Find the matching end for this if statement
    let end_pos = find_matching_end(query)?;
    
    // Find the keywords within this if block
    let then_pos = query.find(" then ");
    
    if then_pos.is_none() {
        return Err(JsonQueryError::InvalidQuery("Missing 'then' in conditional".to_string()));
    }
    
    let then_pos = then_pos.unwrap();
    
    // Find else within this if block (before the matching end)
    let else_pos = find_else_in_block(&query[..end_pos]);
    
    // Extract the parts
    let condition = query[3..then_pos].trim(); // Skip "if "
    
    let (then_expr, else_expr) = if let Some(else_pos) = else_pos {
        if else_pos < then_pos || else_pos > end_pos {
            return Err(JsonQueryError::InvalidQuery("Invalid if-then-else structure".to_string()));
        }
        let then_expr = query[then_pos + 6..else_pos].trim(); // Skip " then "
        let else_expr = query[else_pos + 5..end_pos - 4].trim(); // Skip "else " (5 chars) and " end" (4 chars)

        (then_expr, Some(else_expr))
    } else {
        let then_expr = query[then_pos + 6..end_pos - 3].trim(); // Skip " then " and remove " end"
        (then_expr, None)
    };
    
    // Evaluate the condition
    let condition_result = functions::evaluate_condition(executor, data, condition)?;


    
    // Execute the appropriate branch
    if condition_result {
        execute_expression(executor, data, then_expr)
    } else if let Some(else_expr) = else_expr {
        execute_expression(executor, data, else_expr)
    } else {
        // No else branch and condition is false - return null
        Ok(serde_json::Value::Null)
    }
}