use super::{JsonQueryError, JsonQueryExecutor, parser};
use serde_json;

pub fn execute_arithmetic(executor: &JsonQueryExecutor, data: &serde_json::Value, expression: &str) -> Result<serde_json::Value, JsonQueryError> {

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
                            return Err(JsonQueryError::ExecutionError("Division by zero".to_string()));
                        }
                        l_f64 / r_f64
                    }
                    '%' => {
                        if r_f64 == 0.0 {
                            return Err(JsonQueryError::ExecutionError("Modulo by zero".to_string()));
                        }
                        l_f64 % r_f64
                    }
                    _ => return Err(JsonQueryError::InvalidQuery(format!("Unknown operator: {}", op_char)))
                };
                
                Ok(serde_json::Value::Number(
                    serde_json::Number::from_f64(result)
                        .ok_or_else(|| JsonQueryError::ExecutionError("Invalid numeric result".to_string()))?
                ))
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
            _ => Err(JsonQueryError::ExecutionError(
                format!("Cannot perform {} operation on given types", op_char)
            ))
        }
    } else {
        // No operator found, this should be a single operand
        let operand = expression.trim();
        if operand.starts_with('.') {
            // It's a path query
            executor.execute_query(data, operand)
        } else if operand == "-" {
            // Special case for unary minus
            return execute_arithmetic(executor, data, operand);
        } else {
            // It's a literal value
            parser::parse_value(operand)
        }
    }
}