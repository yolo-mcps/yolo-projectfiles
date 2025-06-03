use super::{JsonQueryError, JsonQueryExecutor, parser};
use serde_json;

fn find_operator_outside_parens(s: &str, op: &str) -> Option<usize> {
    let mut paren_depth: i32 = 0;
    let mut in_string = false;
    let mut escape_next = false;
    let chars: Vec<char> = s.chars().collect();
    let op_chars: Vec<char> = op.chars().collect();
    
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
            _ if !in_string && paren_depth == 0 => {
                // Check if we have the operator at this position
                if i + op_chars.len() <= chars.len() {
                    let mut matches = true;
                    for j in 0..op_chars.len() {
                        if chars[i + j] != op_chars[j] {
                            matches = false;
                            break;
                        }
                    }
                    if matches {
                        return Some(i);
                    }
                }
            }
            _ => {}
        }
    }
    
    None
}

pub fn execute_array_operation(executor: &JsonQueryExecutor, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, JsonQueryError> {
    let query = query.trim();
    
    // Handle .[] (iterate array elements)
    if query == ".[]" || query == "[]" {
        if let serde_json::Value::Array(_arr) = data {
            // For array iteration, we return the array as-is 
            // In a real jq, this would stream elements, but we'll return the array
            return Ok(data.clone());
        } else {
            return Err(JsonQueryError::ExecutionError(
                "Cannot iterate over non-array value".to_string()
            ));
        }
    }
    
    // Handle map() operation
    if query.starts_with("map(") && query.ends_with(')') {
        let inner = &query[4..query.len()-1];
        return execute_map_operation(executor, data, inner);
    }
    
    // Handle select() operation
    if query.starts_with("select(") && query.ends_with(')') {
        let inner = &query[7..query.len()-1];
        return execute_select_operation(executor, data, inner);
    }
    
    // Handle array element iteration with path (.items[] or .users[].name)
    if query.contains("[]") {
        return handle_array_iterator_syntax(executor, data, query);
    }
    
    Err(JsonQueryError::InvalidQuery(
        format!("Unsupported array operation: {}", query)
    ))
}

fn execute_map_operation(executor: &JsonQueryExecutor, data: &serde_json::Value, expression: &str) -> Result<serde_json::Value, JsonQueryError> {
    if let serde_json::Value::Array(arr) = data {
        let mut results = Vec::new();
        for item in arr {
            let result = executor.execute_query(item, expression)?;
            results.push(result);
        }
        Ok(serde_json::Value::Array(results))
    } else {
        Err(JsonQueryError::ExecutionError(
            "map() can only be applied to arrays".to_string()
        ))
    }
}

fn execute_select_operation(executor: &JsonQueryExecutor, data: &serde_json::Value, condition: &str) -> Result<serde_json::Value, JsonQueryError> {
    if let serde_json::Value::Array(arr) = data {
        let mut results = Vec::new();
        for item in arr {
            if evaluate_condition(executor, item, condition)? {
                results.push(item.clone());
            }
        }
        Ok(serde_json::Value::Array(results))
    } else {
        // For non-arrays, select acts as a filter on the single value
        if evaluate_condition(executor, data, condition)? {
            Ok(data.clone())
        } else {
            Ok(serde_json::Value::Null)
        }
    }
}

pub fn evaluate_condition(executor: &JsonQueryExecutor, data: &serde_json::Value, condition: &str) -> Result<bool, JsonQueryError> {
    let condition = condition.trim();
    
    // Handle parentheses first
    if condition.starts_with('(') && condition.ends_with(')') {
        return evaluate_condition(executor, data, &condition[1..condition.len()-1]);
    }
    
    // Check for boolean operators BEFORE comparisons
    // Handle OR (lower precedence) - find rightmost OR outside parentheses
    if let Some(or_pos) = find_operator_outside_parens(condition, " or ") {
        let left = &condition[..or_pos];
        let right = &condition[or_pos + 4..]; // Skip " or "
        let left_result = evaluate_condition(executor, data, left.trim())?;
        let right_result = evaluate_condition(executor, data, right.trim())?;
        return Ok(left_result || right_result);
    }
    
    // Handle AND (higher precedence)
    if let Some(and_pos) = find_operator_outside_parens(condition, " and ") {
        let left = &condition[..and_pos];
        let right = &condition[and_pos + 5..]; // Skip " and "
        let left_result = evaluate_condition(executor, data, left.trim())?;
        let right_result = evaluate_condition(executor, data, right.trim())?;
        return Ok(left_result && right_result);
    }
    
    if condition.starts_with("not ") {
        let inner = condition[4..].trim();
        let result = evaluate_condition(executor, data, inner)?;
        return Ok(!result);
    }
    
    // Handle simple comparisons
    if condition.contains("==") {
        let parts: Vec<&str> = condition.split("==").collect();
        if parts.len() == 2 {
            let left = executor.execute_query(data, parts[0].trim())?;
            let right = parser::parse_value(parts[1].trim())?;
            return Ok(left == right);
        }
    }
    
    if condition.contains("!=") {
        let parts: Vec<&str> = condition.split("!=").collect();
        if parts.len() == 2 {
            let left = executor.execute_query(data, parts[0].trim())?;
            let right = parser::parse_value(parts[1].trim())?;
            return Ok(left != right);
        }
    }
    
    if condition.contains(">=") {
        let parts: Vec<&str> = condition.split(">=").collect();
        if parts.len() == 2 {
            let left = executor.execute_query(data, parts[0].trim())?;
            let right = parser::parse_value(parts[1].trim())?;

            return compare_values(&left, &right, ">=");
        }
    }
    
    if condition.contains("<=") {
        let parts: Vec<&str> = condition.split("<=").collect();
        if parts.len() == 2 {
            let left = executor.execute_query(data, parts[0].trim())?;
            let right = parser::parse_value(parts[1].trim())?;
            return compare_values(&left, &right, "<=");
        }
    }
    
    if condition.contains(">") && !condition.contains(">=") {
        let parts: Vec<&str> = condition.split(">").collect();
        if parts.len() == 2 {
            let left = executor.execute_query(data, parts[0].trim())?;
            let right = parser::parse_value(parts[1].trim())?;
            return compare_values(&left, &right, ">");
        }
    }
    
    if condition.contains("<") && !condition.contains("<=") {
        let parts: Vec<&str> = condition.split("<").collect();
        if parts.len() == 2 {
            let left = executor.execute_query(data, parts[0].trim())?;
            let right = parser::parse_value(parts[1].trim())?;
            return compare_values(&left, &right, "<");
        }
    }
    

    
    // Handle simple value check (truthy/falsy)
    let value = executor.execute_query(data, condition)?;

    Ok(!value.is_null() && value != serde_json::Value::Bool(false))
}

fn compare_values(left: &serde_json::Value, right: &serde_json::Value, op: &str) -> Result<bool, JsonQueryError> {
    match (left, right) {
        (serde_json::Value::Number(l), serde_json::Value::Number(r)) => {
            let l_f64 = l.as_f64().unwrap_or(0.0);
            let r_f64 = r.as_f64().unwrap_or(0.0);
            match op {
                ">" => Ok(l_f64 > r_f64),
                "<" => Ok(l_f64 < r_f64),
                ">=" => Ok(l_f64 >= r_f64),
                "<=" => Ok(l_f64 <= r_f64),
                _ => Err(JsonQueryError::InvalidQuery(format!("Unknown operator: {}", op)))
            }
        }
        (serde_json::Value::String(l), serde_json::Value::String(r)) => {
            match op {
                ">" => Ok(l > r),
                "<" => Ok(l < r),
                ">=" => Ok(l >= r),
                "<=" => Ok(l <= r),
                _ => Err(JsonQueryError::InvalidQuery(format!("Unknown operator: {}", op)))
            }
        }
        _ => Ok(false)
    }
}

fn handle_array_iterator_syntax(executor: &JsonQueryExecutor, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, JsonQueryError> {
    if let Some(bracket_pos) = query.find("[]") {
        let before_bracket = &query[..bracket_pos];
        let after_bracket = &query[bracket_pos + 2..];
        
        // Get the array
        let array_value = if before_bracket.is_empty() || before_bracket == "." {
            data.clone()
        } else {
            executor.execute_query(data, before_bracket)?
        };
        
        if let serde_json::Value::Array(arr) = array_value {
            if after_bracket.is_empty() {
                // Just return the array elements
                Ok(serde_json::Value::Array(arr))
            } else {
                // Apply the rest of the query to each element
                let mut results = Vec::new();
                for item in arr {
                    let item_result = executor.execute_query(&item, after_bracket)?;
                    results.push(item_result);
                }
                Ok(serde_json::Value::Array(results))
            }
        } else {
            Err(JsonQueryError::ExecutionError(
                format!("Cannot iterate with [] on non-array value")
            ))
        }
    } else {
        Err(JsonQueryError::InvalidQuery(
            "Invalid array iterator syntax".to_string()
        ))
    }
}

pub fn execute_builtin_function(data: &serde_json::Value, query: &str) -> Result<serde_json::Value, JsonQueryError> {
    let query = query.trim();
    
    // Handle keys function
    if query == "keys" || query == ".keys" {
        match data {
            serde_json::Value::Object(map) => {
                let mut keys: Vec<String> = map.keys().cloned().collect();
                keys.sort();
                let json_keys: Vec<serde_json::Value> = keys.into_iter()
                    .map(serde_json::Value::String)
                    .collect();
                Ok(serde_json::Value::Array(json_keys))
            }
            serde_json::Value::Array(arr) => {
                // For arrays, keys returns indices
                let indices: Vec<serde_json::Value> = (0..arr.len())
                    .map(|i| serde_json::Value::Number(serde_json::Number::from(i)))
                    .collect();
                Ok(serde_json::Value::Array(indices))
            }
            _ => Err(JsonQueryError::ExecutionError(
                "keys can only be applied to objects or arrays".to_string()
            ))
        }
    }
    // Handle values function
    else if query == "values" || query == ".values" {
        match data {
            serde_json::Value::Object(map) => {
                let values: Vec<serde_json::Value> = map.values().cloned().collect();
                Ok(serde_json::Value::Array(values))
            }
            _ => Err(JsonQueryError::ExecutionError(
                "values can only be applied to objects".to_string()
            ))
        }
    }
    // Handle length function
    else if query == "length" || query == ".length" {
        match data {
            serde_json::Value::String(s) => Ok(serde_json::Value::Number(serde_json::Number::from(s.len()))),
            serde_json::Value::Array(arr) => Ok(serde_json::Value::Number(serde_json::Number::from(arr.len()))),
            serde_json::Value::Object(map) => Ok(serde_json::Value::Number(serde_json::Number::from(map.len()))),
            serde_json::Value::Null => Ok(serde_json::Value::Number(serde_json::Number::from(0))),
            _ => Err(JsonQueryError::ExecutionError("length can only be applied to strings, arrays, objects, or null".to_string()))
        }
    }
    // Handle type function
    else if query == "type" || query == ".type" {
        let type_name = match data {
            serde_json::Value::Null => "null",
            serde_json::Value::Bool(_) => "boolean",
            serde_json::Value::Number(_) => "number",
            serde_json::Value::String(_) => "string",
            serde_json::Value::Array(_) => "array",
            serde_json::Value::Object(_) => "object",
        };
        Ok(serde_json::Value::String(type_name.to_string()))
    }
    // Handle to_entries function
    else if query == "to_entries" || query == ".to_entries" {
        match data {
            serde_json::Value::Object(map) => {
                let entries: Vec<serde_json::Value> = map.iter()
                    .map(|(k, v)| {
                        let mut entry = serde_json::Map::new();
                        entry.insert("key".to_string(), serde_json::Value::String(k.clone()));
                        entry.insert("value".to_string(), v.clone());
                        serde_json::Value::Object(entry)
                    })
                    .collect();
                Ok(serde_json::Value::Array(entries))
            }
            _ => Err(JsonQueryError::ExecutionError(
                "to_entries can only be applied to objects".to_string()
            ))
        }
    }
    // Handle from_entries function
    else if query == "from_entries" || query == ".from_entries" {
        match data {
            serde_json::Value::Array(arr) => {
                let mut result_map = serde_json::Map::new();
                for item in arr {
                    if let serde_json::Value::Object(entry) = item {
                        if let (Some(key_val), Some(value_val)) = (entry.get("key"), entry.get("value")) {
                            if let serde_json::Value::String(key) = key_val {
                                result_map.insert(key.clone(), value_val.clone());
                            }
                        }
                    }
                }
                Ok(serde_json::Value::Object(result_map))
            }
            _ => Err(JsonQueryError::ExecutionError(
                "from_entries can only be applied to arrays".to_string()
            ))
        }
    }
    else {
        // Handle piped builtin functions
        if let Some(pipe_pos) = query.rfind(" | ") {
            let _before_pipe = &query[..pipe_pos];
            let function = &query[pipe_pos + 3..];
            
            // Recursively handle the function
            execute_builtin_function(data, function)
        } else {
            Err(JsonQueryError::InvalidQuery(
                format!("Unknown built-in function: {}", query)
            ))
        }
    }
}

pub fn execute_string_function(_executor: &JsonQueryExecutor, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, JsonQueryError> {
    let query = query.trim();
    
    // Handle simple string functions that don't require evaluation
    if query == "tostring" || query == ".tostring" {
        match data {
            serde_json::Value::String(s) => Ok(serde_json::Value::String(s.clone())),
            serde_json::Value::Number(n) => Ok(serde_json::Value::String(n.to_string())),
            serde_json::Value::Bool(b) => Ok(serde_json::Value::String(b.to_string())),
            serde_json::Value::Null => Ok(serde_json::Value::String("null".to_string())),
            _ => Ok(serde_json::Value::String(serde_json::to_string(data).unwrap_or_default()))
        }
    }
    else if query == "tonumber" || query == ".tonumber" {
        match data {
            serde_json::Value::String(s) => {
                if let Ok(n) = s.parse::<f64>() {
                    Ok(serde_json::Value::Number(serde_json::Number::from_f64(n).unwrap_or(serde_json::Number::from(0))))
                } else {
                    Err(JsonQueryError::ExecutionError(format!("Cannot parse '{}' as number", s)))
                }
            }
            serde_json::Value::Number(n) => Ok(serde_json::Value::Number(n.clone())),
            _ => Err(JsonQueryError::ExecutionError("tonumber can only be applied to strings or numbers".to_string()))
        }
    }
    else if query == "trim" || query == ".trim" {
        match data {
            serde_json::Value::String(s) => Ok(serde_json::Value::String(s.trim().to_string())),
            _ => Err(JsonQueryError::ExecutionError("trim can only be applied to strings".to_string()))
        }
    }
    else if query == "ascii_downcase" || query == ".ascii_downcase" {
        match data {
            serde_json::Value::String(s) => Ok(serde_json::Value::String(s.to_ascii_lowercase())),
            _ => Err(JsonQueryError::ExecutionError("ascii_downcase can only be applied to strings".to_string()))
        }
    }
    else if query == "ascii_upcase" || query == ".ascii_upcase" {
        match data {
            serde_json::Value::String(s) => Ok(serde_json::Value::String(s.to_ascii_uppercase())),
            _ => Err(JsonQueryError::ExecutionError("ascii_upcase can only be applied to strings".to_string()))
        }
    }
    else if query.starts_with("split(") && query.ends_with(')') {
        let arg = &query[6..query.len()-1];
        let separator = parser::parse_string_arg(arg)?;
        
        match data {
            serde_json::Value::String(s) => {
                let parts: Vec<serde_json::Value> = s.split(&separator)
                    .map(|p| serde_json::Value::String(p.to_string()))
                    .collect();
                Ok(serde_json::Value::Array(parts))
            }
            _ => Err(JsonQueryError::ExecutionError("split can only be applied to strings".to_string()))
        }
    }
    else if query.starts_with("join(") && query.ends_with(')') {
        let arg = &query[5..query.len()-1];
        let separator = parser::parse_string_arg(arg)?;
        
        match data {
            serde_json::Value::Array(arr) => {
                let strings: Result<Vec<String>, _> = arr.iter()
                    .map(|v| match v {
                        serde_json::Value::String(s) => Ok(s.clone()),
                        serde_json::Value::Number(n) => Ok(n.to_string()),
                        serde_json::Value::Bool(b) => Ok(b.to_string()),
                        serde_json::Value::Null => Ok("null".to_string()),
                        _ => Err(JsonQueryError::ExecutionError("join can only process arrays of strings or primitives".to_string()))
                    })
                    .collect();
                
                match strings {
                    Ok(strs) => Ok(serde_json::Value::String(strs.join(&separator))),
                    Err(e) => Err(e)
                }
            }
            _ => Err(JsonQueryError::ExecutionError("join can only be applied to arrays".to_string()))
        }
    }
    else if query.starts_with("contains(") && query.ends_with(')') {
        let arg = &query[9..query.len()-1];
        let search = parser::parse_string_arg(arg)?;
        
        match data {
            serde_json::Value::String(s) => Ok(serde_json::Value::Bool(s.contains(&search))),
            _ => Err(JsonQueryError::ExecutionError("contains can only be applied to strings".to_string()))
        }
    }
    else if query.starts_with("startswith(") && query.ends_with(')') {
        let arg = &query[11..query.len()-1];
        let prefix = parser::parse_string_arg(arg)?;
        
        match data {
            serde_json::Value::String(s) => Ok(serde_json::Value::Bool(s.starts_with(&prefix))),
            _ => Err(JsonQueryError::ExecutionError("startswith can only be applied to strings".to_string()))
        }
    }
    else if query.starts_with("endswith(") && query.ends_with(')') {
        let arg = &query[9..query.len()-1];
        let suffix = parser::parse_string_arg(arg)?;
        
        match data {
            serde_json::Value::String(s) => Ok(serde_json::Value::Bool(s.ends_with(&suffix))),
            _ => Err(JsonQueryError::ExecutionError("endswith can only be applied to strings".to_string()))
        }
    }
    else {
        Err(JsonQueryError::InvalidQuery(format!("Unknown string function: {}", query)))
    }
}