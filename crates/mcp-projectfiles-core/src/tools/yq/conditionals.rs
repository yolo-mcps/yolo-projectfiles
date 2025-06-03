use super::{YamlQueryError, YamlQueryExecutor};
use serde_json;

fn find_matching_end(query: &str) -> Result<usize, YamlQueryError> {
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
    
    Err(YamlQueryError::InvalidQuery("Missing 'end' in conditional".to_string()))
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

fn execute_expression(executor: &YamlQueryExecutor, data: &serde_json::Value, expr: &str) -> Result<serde_json::Value, YamlQueryError> {
    let expr = expr.trim();
    
    // Check if it's an object construction (before trying to parse as JSON)
    if expr.starts_with('{') && expr.ends_with('}') {
        return executor.execute(data, expr);
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
    executor.execute(data, expr)
}

pub fn execute_if_then_else(executor: &YamlQueryExecutor, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
    // Parse if-then-else structure: if CONDITION then EXPR else EXPR end
    // Also support: if CONDITION then EXPR end (without else)
    
    if !query.starts_with("if ") {
        return Err(YamlQueryError::InvalidQuery("Conditional must start with 'if'".to_string()));
    }
    
    // Find the matching end for this if statement
    let end_pos = find_matching_end(query)?;
    
    // Find the keywords within this if block
    let then_pos = query.find(" then ");
    
    if then_pos.is_none() {
        return Err(YamlQueryError::InvalidQuery("Missing 'then' in conditional".to_string()));
    }
    
    let then_pos = then_pos.unwrap();
    
    // Find else within this if block (before the matching end)
    let else_pos = find_else_in_block(&query[..end_pos]);
    
    // Extract the parts
    let condition = query[3..then_pos].trim(); // Skip "if "
    
    let (then_expr, else_expr) = if let Some(else_pos) = else_pos {
        if else_pos < then_pos || else_pos > end_pos {
            return Err(YamlQueryError::InvalidQuery("Invalid if-then-else structure".to_string()));
        }
        let then_expr = query[then_pos + 6..else_pos].trim(); // Skip " then "
        let else_expr = query[else_pos + 5..end_pos - 4].trim(); // Skip "else " (5 chars) and " end" (4 chars)

        (then_expr, Some(else_expr))
    } else {
        let then_expr = query[then_pos + 6..end_pos - 3].trim(); // Skip " then " and remove " end"
        (then_expr, None)
    };
    
    // Evaluate the condition
    let condition_result = evaluate_condition(executor, data, condition)?;
    
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

pub fn evaluate_condition(executor: &YamlQueryExecutor, data: &serde_json::Value, condition: &str) -> Result<bool, YamlQueryError> {
    let condition = condition.trim();
    
    // Handle comparison operators
    let comparison_ops = [" == ", " != ", " >= ", " <= ", " > ", " < "];
    
    for op in &comparison_ops {
        if let Some(pos) = condition.find(op) {
            let left_expr = condition[..pos].trim();
            let right_expr = condition[pos + op.len()..].trim();
            
            let left_val = executor.execute_or_literal(data, left_expr)?;
            let right_val = execute_or_literal(executor, data, right_expr)?;
            
            let result = match *op {
                " == " => left_val == right_val,
                " != " => left_val != right_val,
                " > " => compare_values(&left_val, &right_val)? == std::cmp::Ordering::Greater,
                " < " => compare_values(&left_val, &right_val)? == std::cmp::Ordering::Less,
                " >= " => {
                    let ord = compare_values(&left_val, &right_val)?;
                    ord == std::cmp::Ordering::Greater || ord == std::cmp::Ordering::Equal
                }
                " <= " => {
                    let ord = compare_values(&left_val, &right_val)?;
                    ord == std::cmp::Ordering::Less || ord == std::cmp::Ordering::Equal
                }
                _ => unreachable!()
            };
            
            return Ok(result);
        }
    }
    
    // Handle logical operators
    if condition.starts_with("not ") {
        let inner_condition = &condition[4..].trim();
        let inner_result = evaluate_condition(executor, data, inner_condition)?;
        return Ok(!inner_result);
    }
    
    if let Some(pos) = condition.find(" and ") {
        let left_condition = condition[..pos].trim();
        let right_condition = condition[pos + 5..].trim();
        
        let left_result = evaluate_condition(executor, data, left_condition)?;
        if !left_result {
            return Ok(false); // Short-circuit evaluation
        }
        
        let right_result = evaluate_condition(executor, data, right_condition)?;
        return Ok(right_result);
    }
    
    if let Some(pos) = condition.find(" or ") {
        let left_condition = condition[..pos].trim();
        let right_condition = condition[pos + 4..].trim();
        
        let left_result = evaluate_condition(executor, data, left_condition)?;
        if left_result {
            return Ok(true); // Short-circuit evaluation
        }
        
        let right_result = evaluate_condition(executor, data, right_condition)?;
        return Ok(right_result);
    }
    
    // If no operators found, evaluate the condition as an expression and check truthiness
    let result = executor.execute(data, condition)?;
    Ok(is_truthy(&result))
}

fn compare_values(left: &serde_json::Value, right: &serde_json::Value) -> Result<std::cmp::Ordering, YamlQueryError> {
    use std::cmp::Ordering;
    
    match (left, right) {
        (serde_json::Value::Number(l), serde_json::Value::Number(r)) => {
            let l_f64 = l.as_f64().unwrap_or(0.0);
            let r_f64 = r.as_f64().unwrap_or(0.0);
            Ok(l_f64.partial_cmp(&r_f64).unwrap_or(Ordering::Equal))
        }
        (serde_json::Value::String(l), serde_json::Value::String(r)) => {
            Ok(l.cmp(r))
        }
        (serde_json::Value::Bool(l), serde_json::Value::Bool(r)) => {
            Ok(l.cmp(r))
        }
        (serde_json::Value::Array(l), serde_json::Value::Array(r)) => {
            Ok(l.len().cmp(&r.len()))
        }
        (serde_json::Value::Object(l), serde_json::Value::Object(r)) => {
            Ok(l.len().cmp(&r.len()))
        }
        (serde_json::Value::Null, serde_json::Value::Null) => {
            Ok(Ordering::Equal)
        }
        _ => {
            // Convert to strings for comparison if types don't match
            let l_str = value_to_string(left);
            let r_str = value_to_string(right);
            Ok(l_str.cmp(&r_str))
        }
    }
}

fn is_truthy(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::Number(n) => n.as_f64() != Some(0.0),
        serde_json::Value::String(s) => !s.is_empty(),
        serde_json::Value::Array(arr) => !arr.is_empty(),
        serde_json::Value::Object(obj) => !obj.is_empty(),
    }
}

fn value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_string(),
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn execute_or_literal(executor: &YamlQueryExecutor, data: &serde_json::Value, expr: &str) -> Result<serde_json::Value, YamlQueryError> {
    let expr = expr.trim();
    
    // Check if it's a quoted string literal
    if expr.starts_with('"') && expr.ends_with('"') && expr.len() >= 2 {
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
    executor.execute(data, expr)
}