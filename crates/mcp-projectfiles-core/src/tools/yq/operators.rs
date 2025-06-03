use super::{YamlQueryError, YamlQueryExecutor, parser};
use serde_json;

pub fn execute_arithmetic(executor: &YamlQueryExecutor, data: &serde_json::Value, expression: &str) -> Result<serde_json::Value, YamlQueryError> {
    let expression = expression.trim();
    
    // Handle parentheses first
    if expression.starts_with('(') && expression.ends_with(')') {
        let inner = &expression[1..expression.len()-1];
        return execute_arithmetic(executor, data, inner);
    }
    
    // Find the operator with lowest precedence (+ and - before * and /)
    // We scan from right to left to handle left-associativity correctly
    let mut paren_depth = 0;
    let mut brace_depth = 0;
    let mut in_string = false;
    let mut escape_next = false;
    let mut op_pos = None;
    let mut op_char = ' ';
    
    let chars: Vec<char> = expression.chars().collect();
    for i in (0..chars.len()).rev() {
        let ch = chars[i];
        
        // Handle string escaping (scanning backwards)
        if escape_next {
            escape_next = false;
            if ch == '\\' {
                continue;
            }
        }
        
        match ch {
            '"' if !escape_next => in_string = !in_string,
            '\\' if in_string => escape_next = true,
            ')' if !in_string => paren_depth += 1,
            '(' if !in_string => paren_depth -= 1,
            '}' if !in_string => brace_depth += 1,
            '{' if !in_string => brace_depth -= 1,
            '+' | '-' if paren_depth == 0 && brace_depth == 0 && !in_string => {
                // Check if this is actually an operator (not part of a number)
                if i > 0 && expression.chars().nth(i - 1).map_or(false, |c| c.is_whitespace() || c == ')') {
                    op_pos = Some(i);
                    op_char = ch;
                    break;
                }
            }
            _ => {}
        }
    }
    
    // If no + or -, look for * or /
    if op_pos.is_none() {
        paren_depth = 0;
        brace_depth = 0;
        in_string = false;
        escape_next = false;
        
        for i in (0..chars.len()).rev() {
            let ch = chars[i];
            
            // Handle string escaping (scanning backwards)
            if escape_next {
                escape_next = false;
                if ch == '\\' {
                    continue;
                }
            }
            
            match ch {
                '"' if !escape_next => in_string = !in_string,
                '\\' if in_string => escape_next = true,
                ')' if !in_string => paren_depth += 1,
                '(' if !in_string => paren_depth -= 1,
                '}' if !in_string => brace_depth += 1,
                '{' if !in_string => brace_depth -= 1,
                '*' | '/' | '%' if paren_depth == 0 && brace_depth == 0 && !in_string => {
                    op_pos = Some(i);
                    op_char = ch;
                    break;
                }
                _ => {}
            }
        }
    }
    
    if let Some(pos) = op_pos {
        let left_expr = expression[..pos].trim();
        let right_expr = expression[pos + 1..].trim();
        
        let left_val = execute_arithmetic(executor, data, left_expr)?;
        let right_val = execute_arithmetic(executor, data, right_expr)?;
        
        match (left_val, right_val) {
            (serde_json::Value::Number(l), serde_json::Value::Number(r)) => {
                let l_f64 = l.as_f64().unwrap_or(0.0);
                let r_f64 = r.as_f64().unwrap_or(0.0);
                
                let result = match op_char {
                    '+' => l_f64 + r_f64,
                    '-' => l_f64 - r_f64,
                    '*' => l_f64 * r_f64,
                    '/' => {
                        if r_f64 == 0.0 {
                            return Err(YamlQueryError::ExecutionError("Division by zero".to_string()));
                        }
                        l_f64 / r_f64
                    }
                    '%' => {
                        if r_f64 == 0.0 {
                            return Err(YamlQueryError::ExecutionError("Modulo by zero".to_string()));
                        }
                        l_f64 % r_f64
                    }
                    _ => return Err(YamlQueryError::InvalidQuery(format!("Unknown operator: {}", op_char)))
                };
                
                // Try to keep integers as integers when possible
                if result.fract() == 0.0 && result <= i64::MAX as f64 && result >= i64::MIN as f64 {
                    Ok(serde_json::Value::Number(serde_json::Number::from(result as i64)))
                } else {
                    Ok(serde_json::Value::Number(
                        serde_json::Number::from_f64(result)
                            .ok_or_else(|| YamlQueryError::ExecutionError("Invalid numeric result".to_string()))?
                    ))
                }
            }
            (serde_json::Value::String(l), serde_json::Value::String(r)) if op_char == '+' => {
                // String concatenation
                Ok(serde_json::Value::String(format!("{}{}", l, r)))
            }
            (serde_json::Value::Array(mut l), serde_json::Value::Array(r)) if op_char == '+' => {
                // Array concatenation
                l.extend(r);
                Ok(serde_json::Value::Array(l))
            }
            _ => Err(YamlQueryError::ExecutionError(
                format!("Cannot perform {} operation on given types", op_char)
            ))
        }
    } else {
        // No operator found, this should be a single operand
        let operand = expression.trim();
        if operand.starts_with('.') {
            // It's a path query
            executor.execute(data, operand)
        } else if operand == "-" {
            // Special case for unary minus
            return execute_arithmetic(executor, data, operand);
        } else {
            // It's a literal value
            parser::parse_value(operand)
        }
    }
}

pub fn execute_comparison(executor: &YamlQueryExecutor, data: &serde_json::Value, expression: &str) -> Result<serde_json::Value, YamlQueryError> {
    let expression = expression.trim();
    
    // Find comparison operators
    let operators = [" == ", " != ", " >= ", " <= ", " > ", " < "];
    
    for op in &operators {
        if let Some(pos) = expression.find(op) {
            let left_expr = expression[..pos].trim();
            let right_expr = expression[pos + op.len()..].trim();
            
            let left_val = executor.execute_or_literal(data, left_expr)?;
            let right_val = executor.execute_or_literal(data, right_expr)?;
            
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
            
            return Ok(serde_json::Value::Bool(result));
        }
    }
    
    Err(YamlQueryError::InvalidQuery("No comparison operator found".to_string()))
}

pub fn execute_logical(executor: &YamlQueryExecutor, data: &serde_json::Value, expression: &str) -> Result<serde_json::Value, YamlQueryError> {
    let expression = expression.trim();
    
    // Handle 'not' operator first (unary)
    if expression.starts_with("not ") {
        let inner_expr = &expression[4..].trim();
        let inner_result = executor.execute(data, inner_expr)?;
        return Ok(serde_json::Value::Bool(!is_truthy(&inner_result)));
    }
    
    // Handle 'and' operator
    if let Some(pos) = expression.find(" and ") {
        let left_expr = expression[..pos].trim();
        let right_expr = expression[pos + 5..].trim();
        
        let left_val = executor.execute_or_literal(data, left_expr)?;
        if !is_truthy(&left_val) {
            return Ok(serde_json::Value::Bool(false));
        }
        
        let right_val = executor.execute_or_literal(data, right_expr)?;
        return Ok(serde_json::Value::Bool(is_truthy(&right_val)));
    }
    
    // Handle 'or' operator
    if let Some(pos) = expression.find(" or ") {
        let left_expr = expression[..pos].trim();
        let right_expr = expression[pos + 4..].trim();
        
        let left_val = executor.execute_or_literal(data, left_expr)?;
        if is_truthy(&left_val) {
            return Ok(serde_json::Value::Bool(true));
        }
        
        let right_val = executor.execute_or_literal(data, right_expr)?;
        return Ok(serde_json::Value::Bool(is_truthy(&right_val)));
    }
    
    Err(YamlQueryError::InvalidQuery("No logical operator found".to_string()))
}

// Helper functions
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