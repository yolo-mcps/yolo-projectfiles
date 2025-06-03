use super::{JsonQueryError, JsonQueryExecutor, parser};
use serde_json;
use serde_json::json;

pub fn execute_del_operation(_executor: &JsonQueryExecutor, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, JsonQueryError> {
    let path_str = &query[4..query.len()-1]; // Extract path from del(path)
    
    // Clone the data to work with
    let mut result = data.clone();
    
    // Parse the path to delete
    if path_str.starts_with('.') {
        let path = &path_str[1..]; // Remove leading dot
        delete_at_path(&mut result, path)?;
    } else {
        return Err(JsonQueryError::InvalidQuery(
            format!("del() requires a path starting with '.': {}", path_str)
        ));
    }
    
    Ok(result)
}

fn delete_at_path(data: &mut serde_json::Value, path: &str) -> Result<(), JsonQueryError> {
    if path.is_empty() {
        return Ok(());
    }
    
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_brackets = false;
    let mut chars = path.chars().peekable();
    
    while let Some(ch) = chars.next() {
        match ch {
            '.' if !in_brackets => {
                if !current.is_empty() {
                    parts.push(current.clone());
                    current.clear();
                }
            }
            '[' => {
                if !current.is_empty() {
                    parts.push(current.clone());
                    current.clear();
                }
                in_brackets = true;
            }
            ']' => {
                if in_brackets {
                    parts.push(format!("[{}]", current));
                    current.clear();
                    in_brackets = false;
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    
    // Navigate to the parent and delete the final key
    let mut current_data = data;
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            // This is the final part to delete
            if part.starts_with('[') && part.ends_with(']') {
                // Array index
                let index_str = &part[1..part.len()-1];
                if let Ok(index) = index_str.parse::<usize>() {
                    if let serde_json::Value::Array(arr) = current_data {
                        if index < arr.len() {
                            arr.remove(index);
                        }
                    }
                }
            } else {
                // Object key
                if let serde_json::Value::Object(map) = current_data {
                    map.remove(part);
                }
            }
        } else {
            // Navigate deeper
            if part.starts_with('[') && part.ends_with(']') {
                // Array index
                let index_str = &part[1..part.len()-1];
                if let Ok(index) = index_str.parse::<usize>() {
                    if let serde_json::Value::Array(arr) = current_data {
                        if index < arr.len() {
                            current_data = &mut arr[index];
                        } else {
                            return Ok(()); // Index out of bounds, nothing to delete
                        }
                    } else {
                        return Ok(()); // Not an array, nothing to delete
                    }
                }
            } else {
                // Object key
                if let serde_json::Value::Object(map) = current_data {
                    if let Some(value) = map.get_mut(part) {
                        current_data = value;
                    } else {
                        return Ok(()); // Key doesn't exist, nothing to delete
                    }
                } else {
                    return Ok(()); // Not an object, nothing to delete
                }
            }
        }
    }
    
    Ok(())
}

fn collect_paths(data: &serde_json::Value, current_path: &mut Vec<serde_json::Value>, result: &mut Vec<serde_json::Value>, leaf_only: bool) {
    match data {
        serde_json::Value::Object(map) => {
            if !leaf_only && !current_path.is_empty() {
                result.push(serde_json::Value::Array(current_path.clone()));
            }
            for (key, value) in map {
                current_path.push(serde_json::Value::String(key.clone()));
                collect_paths(value, current_path, result, leaf_only);
                current_path.pop();
            }
        }
        serde_json::Value::Array(arr) => {
            if !leaf_only && !current_path.is_empty() {
                result.push(serde_json::Value::Array(current_path.clone()));
            }
            for (index, value) in arr.iter().enumerate() {
                current_path.push(serde_json::Value::Number(serde_json::Number::from(index)));
                collect_paths(value, current_path, result, leaf_only);
                current_path.pop();
            }
        }
        _ => {
            // Leaf node
            if !current_path.is_empty() {
                result.push(serde_json::Value::Array(current_path.clone()));
            }
        }
    }
}

fn flatten_with_depth(arr: &Vec<serde_json::Value>, depth: usize) -> Vec<serde_json::Value> {
    if depth == 0 {
        return arr.clone();
    }
    
    let mut result = Vec::new();
    for item in arr {
        match item {
            serde_json::Value::Array(inner) => {
                if depth > 1 {
                    result.extend(flatten_with_depth(inner, depth - 1));
                } else {
                    result.extend(inner.clone());
                }
            }
            other => result.push(other.clone()),
        }
    }
    result
}

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
    
    // Handle sort function
    if query == "sort" || query == ".sort" {
        return execute_sort_operation(executor, data, None);
    }
    
    // Handle sort_by function
    if query.starts_with("sort_by(") && query.ends_with(')') {
        let inner = &query[8..query.len()-1];
        return execute_sort_operation(executor, data, Some(inner));
    }
    
    // Handle group_by function
    if query.starts_with("group_by(") && query.ends_with(')') {
        let inner = &query[9..query.len()-1];
        return execute_group_by_operation(executor, data, inner);
    }
    
    // Handle array slicing [start:end]
    if query.starts_with('[') && query.ends_with(']') && query.contains(':') {
        let slice_str = &query[1..query.len()-1];
        let parts: Vec<&str> = slice_str.split(':').collect();
        if parts.len() != 2 {
            return Err(JsonQueryError::InvalidQuery("Invalid slice syntax".to_string()));
        }
        
        if let serde_json::Value::Array(arr) = data {
            let start = if parts[0].is_empty() { 
                0 
            } else { 
                parts[0].parse::<usize>()
                    .map_err(|_| JsonQueryError::InvalidQuery(format!("Invalid start index: {}", parts[0])))?
            };
            
            let end = if parts[1].is_empty() { 
                arr.len() 
            } else { 
                parts[1].parse::<usize>()
                    .map_err(|_| JsonQueryError::InvalidQuery(format!("Invalid end index: {}", parts[1])))?
            };
            
            let slice = arr[start.min(arr.len())..end.min(arr.len())].to_vec();
            return Ok(serde_json::Value::Array(slice));
        } else {
            return Err(JsonQueryError::ExecutionError(
                "Array slicing can only be applied to arrays".to_string()
            ));
        }
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

fn execute_group_by_operation(executor: &JsonQueryExecutor, data: &serde_json::Value, group_by_path: &str) -> Result<serde_json::Value, JsonQueryError> {
    if let serde_json::Value::Array(arr) = data {
        use std::collections::HashMap;
        
        let mut groups: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
        
        for item in arr {
            let key_value = executor.execute_query(item, group_by_path)?;
            let key = match &key_value {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Null => "null".to_string(),
                _ => serde_json::to_string(&key_value).unwrap_or_else(|_| "undefined".to_string()),
            };
            
            groups.entry(key).or_insert_with(Vec::new).push(item.clone());
        }
        
        // Convert HashMap to array of arrays, sorted by key for consistent output
        let mut sorted_keys: Vec<_> = groups.keys().cloned().collect();
        sorted_keys.sort();
        
        let result: Vec<serde_json::Value> = sorted_keys
            .into_iter()
            .map(|key| serde_json::Value::Array(groups.remove(&key).unwrap()))
            .collect();
        
        Ok(serde_json::Value::Array(result))
    } else {
        Err(JsonQueryError::ExecutionError(
            "group_by can only be applied to arrays".to_string()
        ))
    }
}

fn execute_sort_operation(executor: &JsonQueryExecutor, data: &serde_json::Value, sort_expr: Option<&str>) -> Result<serde_json::Value, JsonQueryError> {
    if let serde_json::Value::Array(arr) = data {
        let mut sorted = arr.clone();
        
        if let Some(expr) = sort_expr {
            // sort_by with expression
            let mut sort_pairs: Vec<(serde_json::Value, serde_json::Value)> = Vec::new();
            
            for item in arr {
                let sort_key = executor.execute_query(item, expr)?;
                sort_pairs.push((item.clone(), sort_key));
            }
            
            sort_pairs.sort_by(|a, b| compare_json_values(&a.1, &b.1));
            sorted = sort_pairs.into_iter().map(|(item, _)| item).collect();
        } else {
            // Simple sort
            sorted.sort_by(compare_json_values);
        }
        
        Ok(serde_json::Value::Array(sorted))
    } else {
        Err(JsonQueryError::ExecutionError(
            "sort can only be applied to arrays".to_string()
        ))
    }
}

fn compare_json_values(a: &serde_json::Value, b: &serde_json::Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    use serde_json::Value;
    
    match (a, b) {
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Null, _) => Ordering::Less,
        (_, Value::Null) => Ordering::Greater,
        (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
        (Value::Number(a), Value::Number(b)) => {
            let a_f64 = a.as_f64().unwrap_or(0.0);
            let b_f64 = b.as_f64().unwrap_or(0.0);
            a_f64.partial_cmp(&b_f64).unwrap_or(Ordering::Equal)
        }
        (Value::String(a), Value::String(b)) => a.cmp(b),
        (Value::Array(a), Value::Array(b)) => {
            for (a_elem, b_elem) in a.iter().zip(b.iter()) {
                match compare_json_values(a_elem, b_elem) {
                    Ordering::Equal => continue,
                    other => return other,
                }
            }
            a.len().cmp(&b.len())
        }
        (Value::Object(_), Value::Object(_)) => {
            // Objects are compared by their string representation
            let a_str = serde_json::to_string(a).unwrap_or_default();
            let b_str = serde_json::to_string(b).unwrap_or_default();
            a_str.cmp(&b_str)
        }
        // Different types - order by type priority
        _ => {
            let type_order = |v: &Value| match v {
                Value::Null => 0,
                Value::Bool(_) => 1,
                Value::Number(_) => 2,
                Value::String(_) => 3,
                Value::Array(_) => 4,
                Value::Object(_) => 5,
            };
            type_order(a).cmp(&type_order(b))
        }
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
    // Handle add function - sum array elements
    else if query == "add" || query == ".add" {
        match data {
            serde_json::Value::Array(arr) => {
                if arr.is_empty() {
                    return Ok(serde_json::Value::Null);
                }
                
                // Check if all elements are numbers
                let all_numbers = arr.iter().all(|v| v.is_number());
                if all_numbers {
                    let sum: f64 = arr.iter()
                        .filter_map(|v| v.as_f64())
                        .sum();
                    Ok(serde_json::to_value(sum).unwrap_or(serde_json::Value::Null))
                } else {
                    // Try string concatenation if any strings present
                    let all_strings_or_null = arr.iter().all(|v| v.is_string() || v.is_null());
                    if all_strings_or_null {
                        let result: String = arr.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join("");
                        Ok(serde_json::Value::String(result))
                    } else {
                        Err(JsonQueryError::ExecutionError(
                            "add can only sum numbers or concatenate strings".to_string()
                        ))
                    }
                }
            }
            _ => Err(JsonQueryError::ExecutionError(
                "add can only be applied to arrays".to_string()
            ))
        }
    }
    // Handle min function
    else if query == "min" || query == ".min" {
        match data {
            serde_json::Value::Array(arr) => {
                if arr.is_empty() {
                    return Ok(serde_json::Value::Null);
                }
                
                // Try to find minimum number
                let numbers: Vec<f64> = arr.iter()
                    .filter_map(|v| v.as_f64())
                    .collect();
                
                if !numbers.is_empty() {
                    let min_val = numbers.iter().fold(f64::INFINITY, |a, &b| a.min(b));
                    Ok(serde_json::to_value(min_val).unwrap_or(serde_json::Value::Null))
                } else {
                    // Try strings
                    let strings: Vec<&str> = arr.iter()
                        .filter_map(|v| v.as_str())
                        .collect();
                    
                    if !strings.is_empty() {
                        let min_str = strings.into_iter().min().unwrap();
                        Ok(serde_json::Value::String(min_str.to_string()))
                    } else {
                        Err(JsonQueryError::ExecutionError(
                            "min requires array of numbers or strings".to_string()
                        ))
                    }
                }
            }
            _ => Err(JsonQueryError::ExecutionError(
                "min can only be applied to arrays".to_string()
            ))
        }
    }
    // Handle max function
    else if query == "max" || query == ".max" {
        match data {
            serde_json::Value::Array(arr) => {
                if arr.is_empty() {
                    return Ok(serde_json::Value::Null);
                }
                
                // Try to find maximum number
                let numbers: Vec<f64> = arr.iter()
                    .filter_map(|v| v.as_f64())
                    .collect();
                
                if !numbers.is_empty() {
                    let max_val = numbers.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
                    Ok(serde_json::to_value(max_val).unwrap_or(serde_json::Value::Null))
                } else {
                    // Try strings
                    let strings: Vec<&str> = arr.iter()
                        .filter_map(|v| v.as_str())
                        .collect();
                    
                    if !strings.is_empty() {
                        let max_str = strings.into_iter().max().unwrap();
                        Ok(serde_json::Value::String(max_str.to_string()))
                    } else {
                        Err(JsonQueryError::ExecutionError(
                            "max requires array of numbers or strings".to_string()
                        ))
                    }
                }
            }
            _ => Err(JsonQueryError::ExecutionError(
                "max can only be applied to arrays".to_string()
            ))
        }
    }
    // Handle unique function
    else if query == "unique" || query == ".unique" {
        match data {
            serde_json::Value::Array(arr) => {
                let mut unique_values = Vec::new();
                let mut seen = std::collections::HashSet::new();
                
                for val in arr {
                    let val_str = serde_json::to_string(val).unwrap_or_default();
                    if seen.insert(val_str) {
                        unique_values.push(val.clone());
                    }
                }
                
                // Sort the unique values for consistent output
                unique_values.sort_by(|a, b| {
                    let a_str = serde_json::to_string(a).unwrap_or_default();
                    let b_str = serde_json::to_string(b).unwrap_or_default();
                    a_str.cmp(&b_str)
                });
                
                Ok(serde_json::Value::Array(unique_values))
            }
            _ => Err(JsonQueryError::ExecutionError(
                "unique can only be applied to arrays".to_string()
            ))
        }
    }
    // Handle reverse function
    else if query == "reverse" || query == ".reverse" {
        match data {
            serde_json::Value::Array(arr) => {
                let mut reversed = arr.clone();
                reversed.reverse();
                Ok(serde_json::Value::Array(reversed))
            }
            _ => Err(JsonQueryError::ExecutionError(
                "reverse can only be applied to arrays".to_string()
            ))
        }
    }
    // Handle flatten function
    else if query == "flatten" || query == ".flatten" {
        match data {
            serde_json::Value::Array(arr) => {
                let mut flattened = Vec::new();
                for item in arr {
                    match item {
                        serde_json::Value::Array(inner) => {
                            flattened.extend(inner.clone());
                        }
                        other => flattened.push(other.clone()),
                    }
                }
                Ok(serde_json::Value::Array(flattened))
            }
            _ => Err(JsonQueryError::ExecutionError(
                "flatten can only be applied to arrays".to_string()
            ))
        }
    }
    // Handle flatten with depth
    else if query.starts_with("flatten(") && query.ends_with(')') {
        let depth_str = &query[8..query.len()-1];
        match depth_str.parse::<usize>() {
            Ok(depth) => {
                match data {
                    serde_json::Value::Array(arr) => {
                        Ok(serde_json::Value::Array(flatten_with_depth(arr, depth)))
                    }
                    _ => Err(JsonQueryError::ExecutionError(
                        "flatten can only be applied to arrays".to_string()
                    ))
                }
            }
            Err(_) => Err(JsonQueryError::InvalidQuery(
                format!("Invalid depth parameter for flatten: {}", depth_str)
            ))
        }
    }
    // Handle indices function
    else if query.starts_with("indices(") && query.ends_with(')') {
        let value_str = &query[8..query.len()-1];
        // Parse the value to search for
        let search_value: serde_json::Value = if value_str.starts_with('"') && value_str.ends_with('"') {
            // String value
            serde_json::Value::String(value_str[1..value_str.len()-1].to_string())
        } else if value_str == "true" || value_str == "false" {
            // Boolean value
            serde_json::Value::Bool(value_str == "true")
        } else if value_str == "null" {
            // Null value
            serde_json::Value::Null
        } else if let Ok(num) = value_str.parse::<i64>() {
            // Try integer first
            serde_json::Value::Number(serde_json::Number::from(num))
        } else if let Ok(num) = value_str.parse::<f64>() {
            // Then try float
            serde_json::Number::from_f64(num)
                .map(serde_json::Value::Number)
                .unwrap_or_else(|| serde_json::Value::Null)
        } else {
            return Err(JsonQueryError::InvalidQuery(
                format!("Invalid value for indices: {}", value_str)
            ));
        };
        
        match data {
            serde_json::Value::Array(arr) => {
                let mut indices = Vec::new();
                for (i, item) in arr.iter().enumerate() {
                    if item == &search_value {
                        indices.push(serde_json::Value::Number(serde_json::Number::from(i)));
                    }
                }
                Ok(serde_json::Value::Array(indices))
            }
            serde_json::Value::String(s) => {
                // For strings, find substring occurrences
                if let serde_json::Value::String(search_str) = &search_value {
                    let mut indices = Vec::new();
                    let mut start = 0;
                    while let Some(pos) = s[start..].find(search_str) {
                        indices.push(serde_json::Value::Number(serde_json::Number::from(start + pos)));
                        start = start + pos + 1;
                    }
                    Ok(serde_json::Value::Array(indices))
                } else {
                    Ok(serde_json::Value::Array(vec![]))
                }
            }
            _ => Err(JsonQueryError::ExecutionError(
                "indices can only be applied to arrays or strings".to_string()
            ))
        }
    }
    // Handle has function
    else if query.starts_with("has(") && query.ends_with(')') {
        let key_str = &query[4..query.len()-1];
        // Remove quotes if present
        let key = if key_str.starts_with('"') && key_str.ends_with('"') {
            &key_str[1..key_str.len()-1]
        } else {
            key_str
        };
        
        match data {
            serde_json::Value::Object(map) => {
                Ok(serde_json::Value::Bool(map.contains_key(key)))
            }
            serde_json::Value::Array(arr) => {
                // For arrays, check if index exists
                if let Ok(index) = key.parse::<usize>() {
                    Ok(serde_json::Value::Bool(index < arr.len()))
                } else {
                    Ok(serde_json::Value::Bool(false))
                }
            }
            _ => Ok(serde_json::Value::Bool(false))
        }
    }

    // Handle paths function
    else if query == "paths" || query == ".paths" {
        let mut all_paths = Vec::new();
        collect_paths(data, &mut Vec::new(), &mut all_paths, false);
        Ok(serde_json::Value::Array(all_paths))
    }
    // Handle leaf_paths function
    else if query == "leaf_paths" || query == ".leaf_paths" {
        let mut leaf_paths = Vec::new();
        collect_paths(data, &mut Vec::new(), &mut leaf_paths, true);
        Ok(serde_json::Value::Array(leaf_paths))
    }
    // Handle math functions
    else if query == "floor" || query == ".floor" {
        match data {
            serde_json::Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    Ok(serde_json::to_value(f.floor()).unwrap_or(serde_json::Value::Null))
                } else {
                    Ok(data.clone())
                }
            }
            _ => Err(JsonQueryError::ExecutionError("floor can only be applied to numbers".to_string()))
        }
    }
    else if query == "ceil" || query == ".ceil" {
        match data {
            serde_json::Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    Ok(serde_json::to_value(f.ceil()).unwrap_or(serde_json::Value::Null))
                } else {
                    Ok(data.clone())
                }
            }
            _ => Err(JsonQueryError::ExecutionError("ceil can only be applied to numbers".to_string()))
        }
    }
    else if query == "round" || query == ".round" {
        match data {
            serde_json::Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    Ok(serde_json::to_value(f.round()).unwrap_or(serde_json::Value::Null))
                } else {
                    Ok(data.clone())
                }
            }
            _ => Err(JsonQueryError::ExecutionError("round can only be applied to numbers".to_string()))
        }
    }
    else if query == "abs" || query == ".abs" {
        match data {
            serde_json::Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    Ok(serde_json::to_value(f.abs()).unwrap_or(serde_json::Value::Null))
                } else {
                    Ok(data.clone())
                }
            }
            _ => Err(JsonQueryError::ExecutionError("abs can only be applied to numbers".to_string()))
        }
    }
    // Handle empty function
    else if query == "empty" || query == ".empty" {
        // empty produces no output - we'll use a special marker
        // The JQ tool should handle this specially and produce no output
        Ok(serde_json::Value::Array(vec![]))
    }
    // Handle error function
    else if query.starts_with("error(") && query.ends_with(')') {
        let msg = &query[6..query.len()-1];
        let error_msg = if msg.starts_with('"') && msg.ends_with('"') && msg.len() >= 2 {
            &msg[1..msg.len()-1]
        } else {
            msg
        };
        Err(JsonQueryError::ExecutionError(error_msg.to_string()))
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
    else if query.starts_with("test(") && query.ends_with(')') {
        // test(regex) - test if string matches regex
        let arg = &query[5..query.len()-1];
        let pattern = parser::parse_string_arg(arg)?;
        
        match data {
            serde_json::Value::String(s) => {
                match regex::Regex::new(&pattern) {
                    Ok(re) => Ok(serde_json::Value::Bool(re.is_match(s))),
                    Err(e) => Err(JsonQueryError::InvalidQuery(format!("Invalid regex pattern: {}", e)))
                }
            }
            _ => Err(JsonQueryError::ExecutionError("test can only be applied to strings".to_string()))
        }
    }
    else if query.starts_with("match(") && query.ends_with(')') {
        // match(regex) - return match object with captures
        let arg = &query[6..query.len()-1];
        let pattern = parser::parse_string_arg(arg)?;
        
        match data {
            serde_json::Value::String(s) => {
                match regex::Regex::new(&pattern) {
                    Ok(re) => {
                        if let Some(caps) = re.captures(s) {
                            let mut captures = Vec::new();
                            for i in 0..caps.len() {
                                if let Some(m) = caps.get(i) {
                                    captures.push(json!({
                                        "offset": m.start(),
                                        "length": m.len(),
                                        "string": m.as_str(),
                                        "name": null
                                    }));
                                }
                            }
                            Ok(json!({
                                "offset": caps.get(0).map(|m| m.start()).unwrap_or(0),
                                "length": caps.get(0).map(|m| m.len()).unwrap_or(0),
                                "string": caps.get(0).map(|m| m.as_str()).unwrap_or(""),
                                "captures": captures[1..].to_vec()
                            }))
                        } else {
                            Ok(serde_json::Value::Null)
                        }
                    }
                    Err(e) => Err(JsonQueryError::InvalidQuery(format!("Invalid regex pattern: {}", e)))
                }
            }
            _ => Err(JsonQueryError::ExecutionError("match can only be applied to strings".to_string()))
        }
    }
    else if query.starts_with("ltrimstr(") && query.ends_with(')') {
        // ltrimstr(str) - remove prefix string
        let arg = &query[9..query.len()-1];
        let prefix = parser::parse_string_arg(arg)?;
        
        match data {
            serde_json::Value::String(s) => {
                Ok(serde_json::Value::String(s.strip_prefix(&prefix).unwrap_or(s).to_string()))
            }
            _ => Err(JsonQueryError::ExecutionError("ltrimstr can only be applied to strings".to_string()))
        }
    }
    else if query.starts_with("rtrimstr(") && query.ends_with(')') {
        // rtrimstr(str) - remove suffix string
        let arg = &query[9..query.len()-1];
        let suffix = parser::parse_string_arg(arg)?;
        
        match data {
            serde_json::Value::String(s) => {
                Ok(serde_json::Value::String(s.strip_suffix(&suffix).unwrap_or(s).to_string()))
            }
            _ => Err(JsonQueryError::ExecutionError("rtrimstr can only be applied to strings".to_string()))
        }
    }
    else {
        Err(JsonQueryError::InvalidQuery(format!("Unknown string function: {}", query)))
    }
}