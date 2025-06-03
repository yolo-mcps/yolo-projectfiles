use super::{YamlQueryError, YamlQueryExecutor, parser};
use serde_json;
use serde_json::json;
use regex::Regex;

pub fn execute_del_operation(_executor: &YamlQueryExecutor, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
    let path_str = &query[4..query.len()-1]; // Extract path from del(path)
    
    // Clone the data to work with
    let mut result = data.clone();
    
    // Parse the path to delete
    if path_str.starts_with('.') {
        let path = &path_str[1..]; // Remove leading dot
        delete_at_path(&mut result, path)?;
    } else {
        return Err(YamlQueryError::InvalidQuery(
            format!("del() requires a path starting with '.': {}", path_str)
        ));
    }
    
    Ok(result)
}

fn delete_at_path(data: &mut serde_json::Value, path: &str) -> Result<(), YamlQueryError> {
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
            _ => {
                current.push(ch);
            }
        }
    }
    
    if !current.is_empty() {
        parts.push(current);
    }
    
    if parts.is_empty() {
        return Ok(());
    }
    
    // Navigate to the parent and delete the final component
    let mut current_data = data;
    for i in 0..parts.len() - 1 {
        let part = &parts[i];
        if part.starts_with('[') && part.ends_with(']') {
            // Array index
            let index_str = &part[1..part.len()-1];
            let index: usize = index_str.parse()
                .map_err(|_| YamlQueryError::InvalidQuery(format!("Invalid array index: {}", index_str)))?;
            
            if let serde_json::Value::Array(arr) = current_data {
                if index < arr.len() {
                    current_data = &mut arr[index];
                } else {
                    return Err(YamlQueryError::ExecutionError(format!("Array index {} out of bounds", index)));
                }
            } else {
                return Err(YamlQueryError::ExecutionError("Cannot index non-array value".to_string()));
            }
        } else {
            // Object key
            if let serde_json::Value::Object(obj) = current_data {
                current_data = obj.get_mut(part)
                    .ok_or_else(|| YamlQueryError::ExecutionError(format!("Key '{}' not found", part)))?;
            } else {
                return Err(YamlQueryError::ExecutionError("Cannot access field on non-object value".to_string()));
            }
        }
    }
    
    // Delete the final component
    let final_part = &parts[parts.len() - 1];
    if final_part.starts_with('[') && final_part.ends_with(']') {
        // Array index
        let index_str = &final_part[1..final_part.len()-1];
        let index: usize = index_str.parse()
            .map_err(|_| YamlQueryError::InvalidQuery(format!("Invalid array index: {}", index_str)))?;
        
        if let serde_json::Value::Array(arr) = current_data {
            if index < arr.len() {
                arr.remove(index);
            }
        }
    } else {
        // Object key
        if let serde_json::Value::Object(obj) = current_data {
            obj.remove(final_part);
        }
    }
    
    Ok(())
}

pub fn execute_array_operation(executor: &YamlQueryExecutor, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
    let query = query.trim();
    
    // Handle array iteration []
    if query == "[]" || query.ends_with("[]") {
        if query == "[]" {
            // Simple array iteration
            if let serde_json::Value::Array(arr) = data {
                return Ok(serde_json::Value::Array(arr.clone()));
            } else {
                return Ok(serde_json::Value::Array(vec![]));
            }
        } else {
            // Path followed by array iteration like ".items[]"
            let path = &query[..query.len()-2];
            let intermediate = executor.execute(data, path)?;
            if let serde_json::Value::Array(arr) = intermediate {
                return Ok(serde_json::Value::Array(arr));
            } else {
                return Ok(serde_json::Value::Array(vec![]));
            }
        }
    }
    
    // Handle map()
    if query.starts_with("map(") && query.ends_with(')') {
        return execute_map_operation(executor, data, query);
    }
    
    // Handle select()
    if query.starts_with("select(") && query.ends_with(')') {
        return execute_select_operation(executor, data, query);
    }
    
    // Handle sort and sort_by()
    if query == "sort" {
        return execute_sort_operation(data);
    }
    if query.starts_with("sort_by(") && query.ends_with(')') {
        return execute_sort_by_operation(executor, data, query);
    }
    
    // Handle group_by()
    if query.starts_with("group_by(") && query.ends_with(')') {
        return execute_group_by_operation(executor, data, query);
    }
    
    // Handle array slicing [start:end]
    if query.starts_with('[') && query.ends_with(']') && query.contains(':') {
        return execute_array_slice(data, query);
    }
    
    Err(YamlQueryError::InvalidQuery(format!("Unknown array operation: {}", query)))
}

fn execute_map_operation(executor: &YamlQueryExecutor, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
    let expr = &query[4..query.len()-1]; // Remove "map(" and ")"
    
    if let serde_json::Value::Array(arr) = data {
        let mut results = Vec::new();
        for item in arr {
            let result = executor.execute(item, expr)?;
            // Filter out null values that come from select operations
            if result != serde_json::Value::Null {
                results.push(result);
            }
        }
        Ok(serde_json::Value::Array(results))
    } else {
        Err(YamlQueryError::ExecutionError("map() can only be applied to arrays".to_string()))
    }
}

fn execute_select_operation(executor: &YamlQueryExecutor, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
    let expr = &query[7..query.len()-1]; // Remove "select(" and ")"
    
    if let serde_json::Value::Array(arr) = data {
        let mut results = Vec::new();
        for item in arr {
            let condition_result = executor.execute(item, expr)?;
            if is_truthy(&condition_result) {
                results.push(item.clone());
            }
        }
        Ok(serde_json::Value::Array(results))
    } else {
        // For non-arrays, select acts as a filter
        let condition_result = executor.execute(data, expr)?;
        if is_truthy(&condition_result) {
            Ok(data.clone())
        } else {
            Ok(serde_json::Value::Null)
        }
    }
}

fn execute_sort_operation(data: &serde_json::Value) -> Result<serde_json::Value, YamlQueryError> {
    if let serde_json::Value::Array(arr) = data {
        let mut sorted = arr.clone();
        sorted.sort_by(|a, b| compare_json_values(a, b));
        Ok(serde_json::Value::Array(sorted))
    } else {
        Err(YamlQueryError::ExecutionError("sort can only be applied to arrays".to_string()))
    }
}

fn execute_sort_by_operation(executor: &YamlQueryExecutor, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
    let expr = &query[8..query.len()-1]; // Remove "sort_by(" and ")"
    
    if let serde_json::Value::Array(arr) = data {
        let mut items_with_keys: Vec<(serde_json::Value, serde_json::Value)> = Vec::new();
        
        for item in arr {
            let key = executor.execute(item, expr)?;
            items_with_keys.push((key, item.clone()));
        }
        
        items_with_keys.sort_by(|a, b| compare_json_values(&a.0, &b.0));
        
        let sorted: Vec<serde_json::Value> = items_with_keys.into_iter().map(|(_, item)| item).collect();
        Ok(serde_json::Value::Array(sorted))
    } else {
        Err(YamlQueryError::ExecutionError("sort_by() can only be applied to arrays".to_string()))
    }
}

fn execute_group_by_operation(executor: &YamlQueryExecutor, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
    let expr = &query[9..query.len()-1]; // Remove "group_by(" and ")"
    
    if let serde_json::Value::Array(arr) = data {
        let mut groups: std::collections::HashMap<String, Vec<serde_json::Value>> = std::collections::HashMap::new();
        
        for item in arr {
            let key = executor.execute(item, expr)?;
            let key_str = json_value_to_string(&key);
            groups.entry(key_str).or_insert_with(Vec::new).push(item.clone());
        }
        
        let mut result: Vec<serde_json::Value> = groups.into_values()
            .map(|group| serde_json::Value::Array(group))
            .collect();
            
        // Sort groups by first element of each group for consistent output
        result.sort_by(|a, b| {
            if let (serde_json::Value::Array(arr_a), serde_json::Value::Array(arr_b)) = (a, b) {
                if !arr_a.is_empty() && !arr_b.is_empty() {
                    compare_json_values(&arr_a[0], &arr_b[0])
                } else {
                    std::cmp::Ordering::Equal
                }
            } else {
                std::cmp::Ordering::Equal
            }
        });
        
        Ok(serde_json::Value::Array(result))
    } else {
        Err(YamlQueryError::ExecutionError("group_by() can only be applied to arrays".to_string()))
    }
}

fn execute_array_slice(data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
    let slice_expr = &query[1..query.len()-1]; // Remove [ and ]
    
    if let serde_json::Value::Array(arr) = data {
        let parts: Vec<&str> = slice_expr.split(':').collect();
        if parts.len() != 2 {
            return Err(YamlQueryError::InvalidQuery("Array slice must have format [start:end]".to_string()));
        }
        
        let start = if parts[0].is_empty() {
            0
        } else {
            parts[0].parse::<usize>()
                .map_err(|_| YamlQueryError::InvalidQuery(format!("Invalid start index: {}", parts[0])))?
        };
        
        let end = if parts[1].is_empty() {
            arr.len()
        } else {
            parts[1].parse::<usize>()
                .map_err(|_| YamlQueryError::InvalidQuery(format!("Invalid end index: {}", parts[1])))?
        };
        
        if start <= end && start <= arr.len() {
            let end = std::cmp::min(end, arr.len());
            let slice = arr[start..end].to_vec();
            Ok(serde_json::Value::Array(slice))
        } else {
            Ok(serde_json::Value::Array(vec![]))
        }
    } else {
        Err(YamlQueryError::ExecutionError("Array slice can only be applied to arrays".to_string()))
    }
}

pub fn execute_builtin_function(executor: &YamlQueryExecutor, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
    let query = query.trim();
    
    // Handle functions that might have a leading path
    if query.contains(" | ") {
        let parts: Vec<&str> = query.rsplitn(2, " | ").collect();
        if parts.len() == 2 {
            let path = parts[1].trim();
            let func = parts[0].trim();
            let intermediate = executor.execute(data, path)?;
            return execute_builtin_function(executor, &intermediate, func);
        }
    }
    
    // Remove leading dot if present
    let func_name = if query.starts_with('.') {
        &query[1..]
    } else {
        query
    };
    
    match func_name {
        "keys" => execute_keys(data),
        "values" => execute_values(data),
        "length" => execute_length(data),
        "type" => execute_type(data),
        "empty" => Ok(serde_json::Value::Null),
        "add" => execute_add(data),
        "min" => execute_min(data),
        "max" => execute_max(data),
        "unique" => execute_unique(data),
        "reverse" => execute_reverse(data),
        "flatten" => execute_flatten(data),
        "to_entries" => execute_to_entries(data),
        "from_entries" => execute_from_entries(data),
        "paths" => execute_paths(data),
        "leaf_paths" => execute_leaf_paths(data),
        "floor" => execute_floor(data),
        "ceil" => execute_ceil(data),
        "round" => execute_round(data),
        "abs" => execute_abs(data),
        _ => {
            // Handle functions with arguments
            if func_name.starts_with("has(") && func_name.ends_with(')') {
                execute_has_function(data, func_name)
            } else if func_name.starts_with("indices(") && func_name.ends_with(')') {
                execute_indices_function(data, func_name)
            } else {
                Err(YamlQueryError::InvalidQuery(format!("Unknown function: {}", func_name)))
            }
        }
    }
}

pub fn execute_string_function(executor: &YamlQueryExecutor, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
    let query = query.trim();
    
    // Handle functions that might have a leading path
    if query.contains(" | ") {
        let parts: Vec<&str> = query.rsplitn(2, " | ").collect();
        if parts.len() == 2 {
            let path = parts[1].trim();
            let func = parts[0].trim();
            let intermediate = executor.execute(data, path)?;
            return execute_string_function(executor, &intermediate, func);
        }
    }
    
    // Remove leading dot if present
    let func_query = if query.starts_with('.') {
        &query[1..]
    } else {
        query
    };
    
    if func_query.starts_with("split(") && func_query.ends_with(')') {
        execute_split_function(data, func_query)
    } else if func_query.starts_with("join(") && func_query.ends_with(')') {
        execute_join_function(data, func_query)
    } else if func_query == "trim" {
        execute_trim_function(data)
    } else if func_query.starts_with("contains(") && func_query.ends_with(')') {
        execute_contains_function(data, func_query)
    } else if func_query.starts_with("startswith(") && func_query.ends_with(')') {
        execute_startswith_function(data, func_query)
    } else if func_query.starts_with("endswith(") && func_query.ends_with(')') {
        execute_endswith_function(data, func_query)
    } else if func_query.starts_with("test(") && func_query.ends_with(')') {
        execute_test_function(data, func_query)
    } else if func_query.starts_with("match(") && func_query.ends_with(')') {
        execute_match_function(data, func_query)
    } else if func_query.starts_with("ltrimstr(") && func_query.ends_with(')') {
        execute_ltrimstr_function(data, func_query)
    } else if func_query.starts_with("rtrimstr(") && func_query.ends_with(')') {
        execute_rtrimstr_function(data, func_query)
    } else if func_query == "tostring" {
        execute_tostring_function(data)
    } else if func_query == "tonumber" {
        execute_tonumber_function(data)
    } else if func_query == "ascii_upcase" {
        execute_ascii_upcase_function(data)
    } else if func_query == "ascii_downcase" {
        execute_ascii_downcase_function(data)
    } else {
        Err(YamlQueryError::InvalidQuery(format!("Unknown string function: {}", func_query)))
    }
}

// Simple builtin functions that don't need executor (for piped operations)
#[allow(dead_code)]
pub fn execute_simple_builtin_function(data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
    let query = query.trim();
    
    // Remove leading dot if present
    let func_name = if query.starts_with('.') {
        &query[1..]
    } else {
        query
    };
    
    match func_name {
        "keys" => execute_keys(data),
        "values" => execute_values(data),
        "length" => execute_length(data),
        "type" => execute_type(data),
        "empty" => Ok(serde_json::Value::Null),
        "add" => execute_add(data),
        "min" => execute_min(data),
        "max" => execute_max(data),
        "unique" => execute_unique(data),
        "reverse" => execute_reverse(data),
        "flatten" => execute_flatten(data),
        "to_entries" => execute_to_entries(data),
        "from_entries" => execute_from_entries(data),
        "paths" => execute_paths(data),
        "leaf_paths" => execute_leaf_paths(data),
        "floor" => execute_floor(data),
        "ceil" => execute_ceil(data),
        "round" => execute_round(data),
        "abs" => execute_abs(data),
        _ => {
            // Handle functions with arguments
            if func_name.starts_with("has(") && func_name.ends_with(')') {
                execute_has_function(data, func_name)
            } else if func_name.starts_with("indices(") && func_name.ends_with(')') {
                execute_indices_function(data, func_name)
            } else {
                Err(YamlQueryError::InvalidQuery(format!("Unknown function: {}", func_name)))
            }
        }
    }
}

// Implementation of individual functions
fn execute_keys(data: &serde_json::Value) -> Result<serde_json::Value, YamlQueryError> {
    match data {
        serde_json::Value::Object(obj) => {
            let keys: Vec<serde_json::Value> = obj.keys()
                .map(|k| serde_json::Value::String(k.clone()))
                .collect();
            Ok(serde_json::Value::Array(keys))
        }
        serde_json::Value::Array(arr) => {
            let indices: Vec<serde_json::Value> = (0..arr.len())
                .map(|i| serde_json::Value::Number(serde_json::Number::from(i)))
                .collect();
            Ok(serde_json::Value::Array(indices))
        }
        _ => Ok(serde_json::Value::Array(vec![]))
    }
}

fn execute_values(data: &serde_json::Value) -> Result<serde_json::Value, YamlQueryError> {
    match data {
        serde_json::Value::Object(obj) => {
            let values: Vec<serde_json::Value> = obj.values().cloned().collect();
            Ok(serde_json::Value::Array(values))
        }
        serde_json::Value::Array(arr) => {
            Ok(serde_json::Value::Array(arr.clone()))
        }
        _ => Ok(serde_json::Value::Array(vec![]))
    }
}

fn execute_length(data: &serde_json::Value) -> Result<serde_json::Value, YamlQueryError> {
    let len = match data {
        serde_json::Value::Array(arr) => arr.len(),
        serde_json::Value::Object(obj) => obj.len(),
        serde_json::Value::String(s) => s.len(),
        serde_json::Value::Null => 0,
        _ => 1
    };
    Ok(serde_json::Value::Number(serde_json::Number::from(len)))
}

fn execute_type(data: &serde_json::Value) -> Result<serde_json::Value, YamlQueryError> {
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

fn execute_add(data: &serde_json::Value) -> Result<serde_json::Value, YamlQueryError> {
    if let serde_json::Value::Array(arr) = data {
        if arr.is_empty() {
            return Ok(serde_json::Value::Null);
        }
        
        // Check if all elements are numbers
        if arr.iter().all(|v| v.is_number()) {
            let mut sum = 0.0;
            for val in arr {
                if let Some(n) = val.as_f64() {
                    sum += n;
                }
            }
            if sum.fract() == 0.0 && sum <= i64::MAX as f64 && sum >= i64::MIN as f64 {
                Ok(serde_json::Value::Number(serde_json::Number::from(sum as i64)))
            } else {
                Ok(serde_json::Value::Number(serde_json::Number::from_f64(sum).unwrap_or(serde_json::Number::from(0))))
            }
        }
        // Check if all elements are strings
        else if arr.iter().all(|v| v.is_string()) {
            let concatenated = arr.iter()
                .filter_map(|v| v.as_str())
                .collect::<String>();
            Ok(serde_json::Value::String(concatenated))
        }
        // Check if all elements are arrays
        else if arr.iter().all(|v| v.is_array()) {
            let mut result = Vec::new();
            for val in arr {
                if let serde_json::Value::Array(sub_arr) = val {
                    result.extend(sub_arr.clone());
                }
            }
            Ok(serde_json::Value::Array(result))
        }
        else {
            Err(YamlQueryError::ExecutionError("add requires array of numbers, strings, or arrays".to_string()))
        }
    } else {
        Err(YamlQueryError::ExecutionError("add can only be applied to arrays".to_string()))
    }
}

fn execute_min(data: &serde_json::Value) -> Result<serde_json::Value, YamlQueryError> {
    if let serde_json::Value::Array(arr) = data {
        if arr.is_empty() {
            return Ok(serde_json::Value::Null);
        }
        
        let mut min_val = &arr[0];
        for val in &arr[1..] {
            if compare_json_values(val, min_val) == std::cmp::Ordering::Less {
                min_val = val;
            }
        }
        Ok(min_val.clone())
    } else {
        Err(YamlQueryError::ExecutionError("min can only be applied to arrays".to_string()))
    }
}

fn execute_max(data: &serde_json::Value) -> Result<serde_json::Value, YamlQueryError> {
    if let serde_json::Value::Array(arr) = data {
        if arr.is_empty() {
            return Ok(serde_json::Value::Null);
        }
        
        let mut max_val = &arr[0];
        for val in &arr[1..] {
            if compare_json_values(val, max_val) == std::cmp::Ordering::Greater {
                max_val = val;
            }
        }
        Ok(max_val.clone())
    } else {
        Err(YamlQueryError::ExecutionError("max can only be applied to arrays".to_string()))
    }
}

fn execute_unique(data: &serde_json::Value) -> Result<serde_json::Value, YamlQueryError> {
    if let serde_json::Value::Array(arr) = data {
        let mut unique_vals = Vec::new();
        let mut seen = std::collections::HashSet::new();
        
        for val in arr {
            let val_str = json_value_to_string(val);
            if seen.insert(val_str) {
                unique_vals.push(val.clone());
            }
        }
        
        Ok(serde_json::Value::Array(unique_vals))
    } else {
        Err(YamlQueryError::ExecutionError("unique can only be applied to arrays".to_string()))
    }
}

fn execute_reverse(data: &serde_json::Value) -> Result<serde_json::Value, YamlQueryError> {
    if let serde_json::Value::Array(arr) = data {
        let mut reversed = arr.clone();
        reversed.reverse();
        Ok(serde_json::Value::Array(reversed))
    } else {
        Err(YamlQueryError::ExecutionError("reverse can only be applied to arrays".to_string()))
    }
}

fn execute_flatten(data: &serde_json::Value) -> Result<serde_json::Value, YamlQueryError> {
    if let serde_json::Value::Array(arr) = data {
        let mut flattened = Vec::new();
        for val in arr {
            if let serde_json::Value::Array(sub_arr) = val {
                flattened.extend(sub_arr.clone());
            } else {
                flattened.push(val.clone());
            }
        }
        Ok(serde_json::Value::Array(flattened))
    } else {
        Err(YamlQueryError::ExecutionError("flatten can only be applied to arrays".to_string()))
    }
}

fn execute_to_entries(data: &serde_json::Value) -> Result<serde_json::Value, YamlQueryError> {
    if let serde_json::Value::Object(obj) = data {
        let entries: Vec<serde_json::Value> = obj.iter()
            .map(|(k, v)| json!({
                "key": k,
                "value": v
            }))
            .collect();
        Ok(serde_json::Value::Array(entries))
    } else {
        Err(YamlQueryError::ExecutionError("to_entries can only be applied to objects".to_string()))
    }
}

fn execute_from_entries(data: &serde_json::Value) -> Result<serde_json::Value, YamlQueryError> {
    if let serde_json::Value::Array(arr) = data {
        let mut obj = serde_json::Map::new();
        for entry in arr {
            if let serde_json::Value::Object(entry_obj) = entry {
                if let (Some(key), Some(value)) = (entry_obj.get("key"), entry_obj.get("value")) {
                    if let Some(key_str) = key.as_str() {
                        obj.insert(key_str.to_string(), value.clone());
                    }
                }
            }
        }
        Ok(serde_json::Value::Object(obj))
    } else {
        Err(YamlQueryError::ExecutionError("from_entries can only be applied to arrays".to_string()))
    }
}

fn execute_paths(data: &serde_json::Value) -> Result<serde_json::Value, YamlQueryError> {
    let mut paths = Vec::new();
    collect_paths(data, &mut Vec::new(), &mut paths);
    Ok(serde_json::Value::Array(paths))
}

fn execute_leaf_paths(data: &serde_json::Value) -> Result<serde_json::Value, YamlQueryError> {
    let mut paths = Vec::new();
    collect_leaf_paths(data, &mut Vec::new(), &mut paths);
    Ok(serde_json::Value::Array(paths))
}

fn collect_paths(value: &serde_json::Value, current_path: &mut Vec<serde_json::Value>, all_paths: &mut Vec<serde_json::Value>) {
    all_paths.push(serde_json::Value::Array(current_path.clone()));
    
    match value {
        serde_json::Value::Object(obj) => {
            for (key, val) in obj {
                current_path.push(serde_json::Value::String(key.clone()));
                collect_paths(val, current_path, all_paths);
                current_path.pop();
            }
        }
        serde_json::Value::Array(arr) => {
            for (index, val) in arr.iter().enumerate() {
                current_path.push(serde_json::Value::Number(serde_json::Number::from(index)));
                collect_paths(val, current_path, all_paths);
                current_path.pop();
            }
        }
        _ => {}
    }
}

fn collect_leaf_paths(value: &serde_json::Value, current_path: &mut Vec<serde_json::Value>, all_paths: &mut Vec<serde_json::Value>) {
    match value {
        serde_json::Value::Object(obj) => {
            if obj.is_empty() {
                all_paths.push(serde_json::Value::Array(current_path.clone()));
            } else {
                for (key, val) in obj {
                    current_path.push(serde_json::Value::String(key.clone()));
                    collect_leaf_paths(val, current_path, all_paths);
                    current_path.pop();
                }
            }
        }
        serde_json::Value::Array(arr) => {
            if arr.is_empty() {
                all_paths.push(serde_json::Value::Array(current_path.clone()));
            } else {
                for (index, val) in arr.iter().enumerate() {
                    current_path.push(serde_json::Value::Number(serde_json::Number::from(index)));
                    collect_leaf_paths(val, current_path, all_paths);
                    current_path.pop();
                }
            }
        }
        _ => {
            all_paths.push(serde_json::Value::Array(current_path.clone()));
        }
    }
}

// Math functions
fn execute_floor(data: &serde_json::Value) -> Result<serde_json::Value, YamlQueryError> {
    if let Some(n) = data.as_f64() {
        Ok(serde_json::Value::Number(serde_json::Number::from(n.floor() as i64)))
    } else {
        Err(YamlQueryError::ExecutionError("floor can only be applied to numbers".to_string()))
    }
}

fn execute_ceil(data: &serde_json::Value) -> Result<serde_json::Value, YamlQueryError> {
    if let Some(n) = data.as_f64() {
        Ok(serde_json::Value::Number(serde_json::Number::from(n.ceil() as i64)))
    } else {
        Err(YamlQueryError::ExecutionError("ceil can only be applied to numbers".to_string()))
    }
}

fn execute_round(data: &serde_json::Value) -> Result<serde_json::Value, YamlQueryError> {
    if let Some(n) = data.as_f64() {
        Ok(serde_json::Value::Number(serde_json::Number::from(n.round() as i64)))
    } else {
        Err(YamlQueryError::ExecutionError("round can only be applied to numbers".to_string()))
    }
}

fn execute_abs(data: &serde_json::Value) -> Result<serde_json::Value, YamlQueryError> {
    if let Some(n) = data.as_f64() {
        let abs_val = n.abs();
        if abs_val.fract() == 0.0 && abs_val <= i64::MAX as f64 {
            Ok(serde_json::Value::Number(serde_json::Number::from(abs_val as i64)))
        } else {
            Ok(serde_json::Value::Number(serde_json::Number::from_f64(abs_val).unwrap_or(serde_json::Number::from(0))))
        }
    } else {
        Err(YamlQueryError::ExecutionError("abs can only be applied to numbers".to_string()))
    }
}

// Functions with arguments
fn execute_has_function(data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
    let arg = &query[4..query.len()-1]; // Remove "has(" and ")"
    let key = parser::parse_string_arg(arg)?;
    
    match data {
        serde_json::Value::Object(obj) => {
            Ok(serde_json::Value::Bool(obj.contains_key(&key)))
        }
        _ => Ok(serde_json::Value::Bool(false))
    }
}

fn execute_indices_function(data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
    let arg = &query[8..query.len()-1]; // Remove "indices(" and ")"
    let search_value = parser::parse_value(arg)?;
    
    if let serde_json::Value::Array(arr) = data {
        let indices: Vec<serde_json::Value> = arr.iter()
            .enumerate()
            .filter_map(|(i, val)| {
                if val == &search_value {
                    Some(serde_json::Value::Number(serde_json::Number::from(i)))
                } else {
                    None
                }
            })
            .collect();
        Ok(serde_json::Value::Array(indices))
    } else {
        Err(YamlQueryError::ExecutionError("indices can only be applied to arrays".to_string()))
    }
}

// String functions
fn execute_split_function(data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
    let arg = &query[6..query.len()-1]; // Remove "split(" and ")"
    let separator = parser::parse_string_arg(arg)?;
    
    if let Some(s) = data.as_str() {
        let parts: Vec<serde_json::Value> = s.split(&separator)
            .map(|part| serde_json::Value::String(part.to_string()))
            .collect();
        Ok(serde_json::Value::Array(parts))
    } else {
        Err(YamlQueryError::ExecutionError("split can only be applied to strings".to_string()))
    }
}

fn execute_join_function(data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
    let arg = &query[5..query.len()-1]; // Remove "join(" and ")"
    let separator = parser::parse_string_arg(arg)?;
    
    if let serde_json::Value::Array(arr) = data {
        let strings: Result<Vec<String>, YamlQueryError> = arr.iter()
            .map(|val| {
                if let Some(s) = val.as_str() {
                    Ok(s.to_string())
                } else {
                    Err(YamlQueryError::ExecutionError("join can only be applied to arrays of strings".to_string()))
                }
            })
            .collect();
        
        let strings = strings?;
        Ok(serde_json::Value::String(strings.join(&separator)))
    } else {
        Err(YamlQueryError::ExecutionError("join can only be applied to arrays".to_string()))
    }
}

fn execute_trim_function(data: &serde_json::Value) -> Result<serde_json::Value, YamlQueryError> {
    if let Some(s) = data.as_str() {
        Ok(serde_json::Value::String(s.trim().to_string()))
    } else {
        Err(YamlQueryError::ExecutionError("trim can only be applied to strings".to_string()))
    }
}

fn execute_contains_function(data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
    let arg = &query[9..query.len()-1]; // Remove "contains(" and ")"
    let search = parser::parse_string_arg(arg)?;
    
    if let Some(s) = data.as_str() {
        Ok(serde_json::Value::Bool(s.contains(&search)))
    } else {
        Err(YamlQueryError::ExecutionError("contains can only be applied to strings".to_string()))
    }
}

fn execute_startswith_function(data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
    let arg = &query[11..query.len()-1]; // Remove "startswith(" and ")"
    let prefix = parser::parse_string_arg(arg)?;
    
    if let Some(s) = data.as_str() {
        Ok(serde_json::Value::Bool(s.starts_with(&prefix)))
    } else {
        Err(YamlQueryError::ExecutionError("startswith can only be applied to strings".to_string()))
    }
}

fn execute_endswith_function(data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
    let arg = &query[9..query.len()-1]; // Remove "endswith(" and ")"
    let suffix = parser::parse_string_arg(arg)?;
    
    if let Some(s) = data.as_str() {
        Ok(serde_json::Value::Bool(s.ends_with(&suffix)))
    } else {
        Err(YamlQueryError::ExecutionError("endswith can only be applied to strings".to_string()))
    }
}

fn execute_test_function(data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
    let arg = &query[5..query.len()-1]; // Remove "test(" and ")"
    let pattern = parser::parse_string_arg(arg)?;
    
    if let Some(s) = data.as_str() {
        match Regex::new(&pattern) {
            Ok(re) => Ok(serde_json::Value::Bool(re.is_match(s))),
            Err(e) => Err(YamlQueryError::ExecutionError(format!("Invalid regex pattern: {}", e)))
        }
    } else {
        Err(YamlQueryError::ExecutionError("test can only be applied to strings".to_string()))
    }
}

fn execute_match_function(data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
    let arg = &query[6..query.len()-1]; // Remove "match(" and ")"
    let pattern = parser::parse_string_arg(arg)?;
    
    if let Some(s) = data.as_str() {
        match Regex::new(&pattern) {
            Ok(re) => {
                if let Some(captures) = re.captures(s) {
                    let matches: Vec<serde_json::Value> = captures.iter()
                        .map(|m| {
                            if let Some(m) = m {
                                serde_json::Value::String(m.as_str().to_string())
                            } else {
                                serde_json::Value::Null
                            }
                        })
                        .collect();
                    Ok(serde_json::Value::Array(matches))
                } else {
                    Ok(serde_json::Value::Null)
                }
            }
            Err(e) => Err(YamlQueryError::ExecutionError(format!("Invalid regex pattern: {}", e)))
        }
    } else {
        Err(YamlQueryError::ExecutionError("match can only be applied to strings".to_string()))
    }
}

fn execute_ltrimstr_function(data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
    let arg = &query[9..query.len()-1]; // Remove "ltrimstr(" and ")"
    let prefix = parser::parse_string_arg(arg)?;
    
    if let Some(s) = data.as_str() {
        let result = if s.starts_with(&prefix) {
            &s[prefix.len()..]
        } else {
            s
        };
        Ok(serde_json::Value::String(result.to_string()))
    } else {
        Err(YamlQueryError::ExecutionError("ltrimstr can only be applied to strings".to_string()))
    }
}

fn execute_rtrimstr_function(data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
    let arg = &query[9..query.len()-1]; // Remove "rtrimstr(" and ")"
    let suffix = parser::parse_string_arg(arg)?;
    
    if let Some(s) = data.as_str() {
        let result = if s.ends_with(&suffix) {
            &s[..s.len() - suffix.len()]
        } else {
            s
        };
        Ok(serde_json::Value::String(result.to_string()))
    } else {
        Err(YamlQueryError::ExecutionError("rtrimstr can only be applied to strings".to_string()))
    }
}

fn execute_tostring_function(data: &serde_json::Value) -> Result<serde_json::Value, YamlQueryError> {
    let result = match data {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_string(),
        _ => serde_json::to_string(data).unwrap_or_default(),
    };
    Ok(serde_json::Value::String(result))
}

fn execute_tonumber_function(data: &serde_json::Value) -> Result<serde_json::Value, YamlQueryError> {
    match data {
        serde_json::Value::Number(_n) => Ok(data.clone()),
        serde_json::Value::String(s) => {
            if let Ok(i) = s.parse::<i64>() {
                Ok(serde_json::Value::Number(serde_json::Number::from(i)))
            } else if let Ok(f) = s.parse::<f64>() {
                Ok(serde_json::Value::Number(serde_json::Number::from_f64(f).unwrap_or(serde_json::Number::from(0))))
            } else {
                Err(YamlQueryError::ExecutionError(format!("Cannot convert '{}' to number", s)))
            }
        }
        _ => Err(YamlQueryError::ExecutionError("tonumber can only be applied to strings or numbers".to_string()))
    }
}

fn execute_ascii_upcase_function(data: &serde_json::Value) -> Result<serde_json::Value, YamlQueryError> {
    if let Some(s) = data.as_str() {
        Ok(serde_json::Value::String(s.to_uppercase()))
    } else {
        Err(YamlQueryError::ExecutionError("ascii_upcase can only be applied to strings".to_string()))
    }
}

fn execute_ascii_downcase_function(data: &serde_json::Value) -> Result<serde_json::Value, YamlQueryError> {
    if let Some(s) = data.as_str() {
        Ok(serde_json::Value::String(s.to_lowercase()))
    } else {
        Err(YamlQueryError::ExecutionError("ascii_downcase can only be applied to strings".to_string()))
    }
}

// Helper functions
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

fn compare_json_values(a: &serde_json::Value, b: &serde_json::Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    
    match (a, b) {
        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
            a.as_f64().partial_cmp(&b.as_f64()).unwrap_or(Ordering::Equal)
        }
        (serde_json::Value::String(a), serde_json::Value::String(b)) => a.cmp(b),
        (serde_json::Value::Bool(a), serde_json::Value::Bool(b)) => a.cmp(b),
        (serde_json::Value::Null, serde_json::Value::Null) => Ordering::Equal,
        _ => json_value_to_string(a).cmp(&json_value_to_string(b))
    }
}

fn json_value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_string(),
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}