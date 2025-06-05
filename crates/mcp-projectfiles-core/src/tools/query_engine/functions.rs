use serde_json::{Value, Map, json};
use super::errors::QueryError;
use super::executor::QueryEngine;

/// Try to execute a built-in function
pub fn try_builtin_function(engine: &QueryEngine, data: &Value, query: &str) -> Result<Option<Value>, QueryError> {
    // Check for function call pattern
    if let Some((func_name, args)) = engine.parser.parse_function_call(query) {
        return execute_function(engine, data, &func_name, &args).map(Some);
    }
    
    // Check for simple built-in functions without parentheses
    match query {
        "keys" => Ok(Some(keys(data)?)),
        "values" => Ok(Some(values(data)?)),
        "length" => Ok(Some(length(data)?)),
        "type" => Ok(Some(type_of(data)?)),
        "reverse" => Ok(Some(reverse(data)?)),
        "sort" => Ok(Some(sort(data)?)),
        "unique" => Ok(Some(unique(data)?)),
        "flatten" => Ok(Some(flatten(data)?)),
        "add" => Ok(Some(add(data)?)),
        "min" => Ok(Some(min(data)?)),
        "max" => Ok(Some(max(data)?)),
        "empty" => Ok(Some(Value::Array(vec![]))),
        "not" => Ok(Some(Value::Bool(!is_truthy(data)))),
        "to_entries" => Ok(Some(to_entries(data)?)),
        "from_entries" => Ok(Some(from_entries(data)?)),
        "floor" => Ok(Some(floor(data)?)),
        "ceil" => Ok(Some(ceil(data)?)),
        "round" => Ok(Some(round(data)?)),
        "abs" => Ok(Some(abs(data)?)),
        "tostring" => Ok(Some(to_string(data)?)),
        "tonumber" => Ok(Some(to_number(data)?)),
        "trim" => Ok(Some(trim(data)?)),
        "ascii_upcase" => Ok(Some(ascii_upcase(data)?)),
        "ascii_downcase" => Ok(Some(ascii_downcase(data)?)),
        "paths" => Ok(Some(paths(data)?)),
        "leaf_paths" => Ok(Some(leaf_paths(data)?)),
        "objects" => Ok(Some(objects(data)?)),
        _ => Ok(None),
    }
}

fn execute_function(engine: &QueryEngine, data: &Value, func_name: &str, args: &str) -> Result<Value, QueryError> {
    match func_name {
        "map" => execute_map(engine, data, args),
        "select" => execute_select(engine, data, args),
        "sort_by" => execute_sort_by(engine, data, args),
        "group_by" => execute_group_by(engine, data, args),
        "has" => execute_has(data, args),
        "contains" => execute_contains(data, args),
        "startswith" => execute_startswith(data, args),
        "endswith" => execute_endswith(data, args),
        "split" => execute_split(data, args),
        "join" => execute_join(data, args),
        "test" => execute_test(data, args),
        "match" => execute_match(data, args),
        "indices" => execute_indices(data, args),
        "index" => execute_index(data, args),
        "rindex" => execute_rindex(data, args),
        "ltrimstr" => execute_ltrimstr(data, args),
        "rtrimstr" => execute_rtrimstr(data, args),
        "flatten" => execute_flatten(data, args),
        "error" => execute_error(args),
        _ => Err(QueryError::FunctionNotFound(func_name.to_string())),
    }
}

// Array functions

fn keys(data: &Value) -> Result<Value, QueryError> {
    match data {
        Value::Object(obj) => {
            let keys: Vec<Value> = obj.keys()
                .map(|k| Value::String(k.clone()))
                .collect();
            Ok(Value::Array(keys))
        }
        Value::Array(arr) => {
            let keys: Vec<Value> = (0..arr.len())
                .map(|i| Value::Number(serde_json::Number::from(i)))
                .collect();
            Ok(Value::Array(keys))
        }
        _ => Err(QueryError::TypeError("keys() requires an object or array".to_string())),
    }
}

fn values(data: &Value) -> Result<Value, QueryError> {
    match data {
        Value::Object(obj) => Ok(Value::Array(obj.values().cloned().collect())),
        Value::Array(arr) => Ok(Value::Array(arr.clone())),
        _ => Err(QueryError::TypeError("values() requires an object or array".to_string())),
    }
}

fn length(data: &Value) -> Result<Value, QueryError> {
    let len = match data {
        Value::String(s) => s.len(),
        Value::Array(arr) => arr.len(),
        Value::Object(obj) => obj.len(),
        Value::Null => 0,
        _ => return Err(QueryError::TypeError("length() requires a string, array, or object".to_string())),
    };
    Ok(Value::Number(serde_json::Number::from(len)))
}

fn type_of(data: &Value) -> Result<Value, QueryError> {
    let type_name = match data {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    };
    Ok(Value::String(type_name.to_string()))
}

fn reverse(data: &Value) -> Result<Value, QueryError> {
    match data {
        Value::Array(arr) => {
            let mut reversed = arr.clone();
            reversed.reverse();
            Ok(Value::Array(reversed))
        }
        Value::String(s) => {
            Ok(Value::String(s.chars().rev().collect()))
        }
        _ => Err(QueryError::TypeError("reverse() requires an array or string".to_string())),
    }
}

fn sort(data: &Value) -> Result<Value, QueryError> {
    match data {
        Value::Array(arr) => {
            let mut sorted = arr.clone();
            sorted.sort_by(|a, b| compare_json_values(a, b));
            Ok(Value::Array(sorted))
        }
        _ => Err(QueryError::TypeError("sort() requires an array".to_string())),
    }
}

fn unique(data: &Value) -> Result<Value, QueryError> {
    match data {
        Value::Array(arr) => {
            let mut seen = std::collections::HashSet::new();
            let mut unique_arr = Vec::new();
            
            for item in arr {
                let key = serde_json::to_string(item).unwrap_or_default();
                if seen.insert(key) {
                    unique_arr.push(item.clone());
                }
            }
            
            Ok(Value::Array(unique_arr))
        }
        _ => Err(QueryError::TypeError("unique() requires an array".to_string())),
    }
}

fn flatten(data: &Value) -> Result<Value, QueryError> {
    match data {
        Value::Array(arr) => {
            let mut flattened = Vec::new();
            for item in arr {
                match item {
                    Value::Array(inner) => flattened.extend(inner.iter().cloned()),
                    other => flattened.push(other.clone()),
                }
            }
            Ok(Value::Array(flattened))
        }
        _ => Err(QueryError::TypeError("flatten() requires an array".to_string())),
    }
}

fn execute_flatten(data: &Value, args: &str) -> Result<Value, QueryError> {
    if args.is_empty() {
        return flatten(data);
    }
    
    // Parse depth argument
    let depth = args.trim().parse::<i32>()
        .map_err(|_| QueryError::InvalidSyntax("flatten() depth must be a number".to_string()))?;
    
    if depth < 0 {
        return Err(QueryError::InvalidSyntax("flatten() depth must be non-negative".to_string()));
    }
    
    flatten_with_depth(data, depth)
}

fn flatten_with_depth(data: &Value, depth: i32) -> Result<Value, QueryError> {
    match data {
        Value::Array(arr) => {
            if depth == 0 {
                return Ok(data.clone());
            }
            
            let mut flattened = Vec::new();
            for item in arr {
                match item {
                    Value::Array(_inner) if depth > 0 => {
                        let inner_flattened = flatten_with_depth(item, depth - 1)?;
                        if let Value::Array(inner_arr) = inner_flattened {
                            flattened.extend(inner_arr);
                        }
                    }
                    other => flattened.push(other.clone()),
                }
            }
            Ok(Value::Array(flattened))
        }
        _ => Err(QueryError::TypeError("flatten() requires an array".to_string())),
    }
}

fn add(data: &Value) -> Result<Value, QueryError> {
    match data {
        Value::Array(arr) => {
            if arr.is_empty() {
                return Ok(Value::Null);
            }
            
            // Check if all elements are numbers
            if arr.iter().all(|v| v.is_number()) {
                let sum = arr.iter()
                    .filter_map(|v| v.as_f64())
                    .sum::<f64>();
                return serde_json::Number::from_f64(sum)
                    .map(Value::Number)
                    .ok_or_else(|| QueryError::ExecutionError("Invalid number result".to_string()));
            }
            
            // Check if all elements are strings
            if arr.iter().all(|v| v.is_string()) {
                let concatenated = arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join("");
                return Ok(Value::String(concatenated));
            }
            
            // Check if all elements are arrays
            if arr.iter().all(|v| v.is_array()) {
                let mut result = Vec::new();
                for item in arr {
                    if let Value::Array(inner) = item {
                        result.extend(inner.iter().cloned());
                    }
                }
                return Ok(Value::Array(result));
            }
            
            Err(QueryError::TypeError("add() requires all elements to be of the same type".to_string()))
        }
        _ => Err(QueryError::TypeError("add() requires an array".to_string())),
    }
}

fn min(data: &Value) -> Result<Value, QueryError> {
    match data {
        Value::Array(arr) => {
            if arr.is_empty() {
                return Ok(Value::Null);
            }
            arr.iter()
                .min_by(|a, b| compare_json_values(a, b))
                .cloned()
                .ok_or_else(|| QueryError::ExecutionError("Failed to find minimum".to_string()))
        }
        _ => Err(QueryError::TypeError("min() requires an array".to_string())),
    }
}

fn max(data: &Value) -> Result<Value, QueryError> {
    match data {
        Value::Array(arr) => {
            if arr.is_empty() {
                return Ok(Value::Null);
            }
            arr.iter()
                .max_by(|a, b| compare_json_values(a, b))
                .cloned()
                .ok_or_else(|| QueryError::ExecutionError("Failed to find maximum".to_string()))
        }
        _ => Err(QueryError::TypeError("max() requires an array".to_string())),
    }
}

fn to_entries(data: &Value) -> Result<Value, QueryError> {
    match data {
        Value::Object(obj) => {
            let entries: Vec<Value> = obj.iter().map(|(k, v)| {
                json!({
                    "key": k,
                    "value": v.clone()
                })
            }).collect();
            Ok(Value::Array(entries))
        }
        _ => Err(QueryError::TypeError("to_entries() requires an object".to_string())),
    }
}

fn from_entries(data: &Value) -> Result<Value, QueryError> {
    match data {
        Value::Array(arr) => {
            let mut obj = Map::new();
            for entry in arr {
                if let Value::Object(entry_obj) = entry {
                    if let (Some(Value::String(key)), Some(value)) = 
                        (entry_obj.get("key"), entry_obj.get("value")) {
                        obj.insert(key.clone(), value.clone());
                    }
                }
            }
            Ok(Value::Object(obj))
        }
        _ => Err(QueryError::TypeError("from_entries() requires an array".to_string())),
    }
}

// Math functions

fn floor(data: &Value) -> Result<Value, QueryError> {
    match data {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                Ok(Value::Number(serde_json::Number::from(f.floor() as i64)))
            } else {
                Ok(data.clone())
            }
        }
        _ => Err(QueryError::TypeError("floor() requires a number".to_string())),
    }
}

fn ceil(data: &Value) -> Result<Value, QueryError> {
    match data {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                Ok(Value::Number(serde_json::Number::from(f.ceil() as i64)))
            } else {
                Ok(data.clone())
            }
        }
        _ => Err(QueryError::TypeError("ceil() requires a number".to_string())),
    }
}

fn round(data: &Value) -> Result<Value, QueryError> {
    match data {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                Ok(Value::Number(serde_json::Number::from(f.round() as i64)))
            } else {
                Ok(data.clone())
            }
        }
        _ => Err(QueryError::TypeError("round() requires a number".to_string())),
    }
}

fn abs(data: &Value) -> Result<Value, QueryError> {
    match data {
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Number(serde_json::Number::from(i.abs())))
            } else if let Some(f) = n.as_f64() {
                serde_json::Number::from_f64(f.abs())
                    .map(Value::Number)
                    .ok_or_else(|| QueryError::ExecutionError("Invalid number result".to_string()))
            } else {
                Ok(data.clone())
            }
        }
        _ => Err(QueryError::TypeError("abs() requires a number".to_string())),
    }
}

// String functions

fn trim(data: &Value) -> Result<Value, QueryError> {
    match data {
        Value::String(s) => Ok(Value::String(s.trim().to_string())),
        _ => Err(QueryError::TypeError("trim() requires a string".to_string())),
    }
}

fn ascii_upcase(data: &Value) -> Result<Value, QueryError> {
    match data {
        Value::String(s) => Ok(Value::String(s.to_ascii_uppercase())),
        _ => Err(QueryError::TypeError("ascii_upcase() requires a string".to_string())),
    }
}

fn ascii_downcase(data: &Value) -> Result<Value, QueryError> {
    match data {
        Value::String(s) => Ok(Value::String(s.to_ascii_lowercase())),
        _ => Err(QueryError::TypeError("ascii_downcase() requires a string".to_string())),
    }
}

fn to_string(data: &Value) -> Result<Value, QueryError> {
    let s = match data {
        Value::String(s) => s.clone(),
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        _ => serde_json::to_string(data)
            .map_err(|e| QueryError::ExecutionError(format!("Failed to convert to string: {}", e)))?,
    };
    Ok(Value::String(s))
}

fn to_number(data: &Value) -> Result<Value, QueryError> {
    match data {
        Value::Number(n) => Ok(Value::Number(n.clone())),
        Value::String(s) => {
            if let Ok(i) = s.parse::<i64>() {
                Ok(Value::Number(serde_json::Number::from(i)))
            } else if let Ok(f) = s.parse::<f64>() {
                serde_json::Number::from_f64(f)
                    .map(Value::Number)
                    .ok_or_else(|| QueryError::ExecutionError("Invalid number".to_string()))
            } else {
                Err(QueryError::TypeError(format!("Cannot convert '{}' to number", s)))
            }
        }
        Value::Bool(b) => Ok(Value::Number(serde_json::Number::from(if *b { 1 } else { 0 }))),
        _ => Err(QueryError::TypeError("Cannot convert to number".to_string())),
    }
}

// Function implementations with arguments

fn execute_map(engine: &QueryEngine, data: &Value, expr: &str) -> Result<Value, QueryError> {
    match data {
        Value::Array(arr) => {
            // Check if the expression is a select() call
            let is_select = expr.trim().starts_with("select(");
            
            let mut results = Vec::new();
            for item in arr {
                match engine.execute(item, expr) {
                    Ok(Value::Null) if is_select => {
                        // Skip null results from select()
                        continue;
                    }
                    Ok(value) => results.push(value),
                    Err(e) => return Err(e),
                }
            }
            Ok(Value::Array(results))
        }
        _ => Err(QueryError::TypeError("map() requires an array".to_string())),
    }
}

fn execute_select(engine: &QueryEngine, data: &Value, expr: &str) -> Result<Value, QueryError> {
    match data {
        Value::Array(arr) => {
            let mut results = Vec::new();
            for item in arr {
                let condition = engine.execute(item, expr)?;
                if is_truthy(&condition) {
                    results.push(item.clone());
                }
            }
            Ok(Value::Array(results))
        }
        _ => {
            // For individual items, check condition and return item or null
            let condition = engine.execute(data, expr)?;
            if is_truthy(&condition) {
                Ok(data.clone())
            } else {
                Ok(Value::Null)
            }
        }
    }
}

fn execute_sort_by(engine: &QueryEngine, data: &Value, expr: &str) -> Result<Value, QueryError> {
    match data {
        Value::Array(arr) => {
            let mut items_with_keys: Vec<(Value, Value)> = Vec::new();
            
            for item in arr {
                let key = engine.execute(item, expr)?;
                items_with_keys.push((item.clone(), key));
            }
            
            items_with_keys.sort_by(|a, b| compare_json_values(&a.1, &b.1));
            
            let sorted: Vec<Value> = items_with_keys.into_iter()
                .map(|(item, _)| item)
                .collect();
            
            Ok(Value::Array(sorted))
        }
        _ => Err(QueryError::TypeError("sort_by() requires an array".to_string())),
    }
}

fn execute_group_by(engine: &QueryEngine, data: &Value, expr: &str) -> Result<Value, QueryError> {
    match data {
        Value::Array(arr) => {
            let mut groups: std::collections::BTreeMap<String, Vec<Value>> = std::collections::BTreeMap::new();
            
            for item in arr {
                let key = engine.execute(item, expr)?;
                let key_str = serde_json::to_string(&key).unwrap_or_default();
                groups.entry(key_str).or_insert_with(Vec::new).push(item.clone());
            }
            
            let result: Vec<Value> = groups.into_values()
                .map(Value::Array)
                .collect();
            
            Ok(Value::Array(result))
        }
        _ => Err(QueryError::TypeError("group_by() requires an array".to_string())),
    }
}

fn execute_has(data: &Value, key: &str) -> Result<Value, QueryError> {
    let key = key.trim_matches('"');
    match data {
        Value::Object(obj) => Ok(Value::Bool(obj.contains_key(key))),
        _ => Ok(Value::Bool(false)),
    }
}

fn execute_contains(data: &Value, needle: &str) -> Result<Value, QueryError> {
    match data {
        Value::String(s) => {
            let needle = needle.trim_matches('"');
            Ok(Value::Bool(s.contains(needle)))
        }
        Value::Array(arr) => {
            let needle_value = serde_json::from_str(needle)
                .unwrap_or_else(|_| Value::String(needle.to_string()));
            Ok(Value::Bool(arr.contains(&needle_value)))
        }
        _ => Err(QueryError::TypeError("contains() requires a string or array".to_string())),
    }
}

fn execute_startswith(data: &Value, prefix: &str) -> Result<Value, QueryError> {
    match data {
        Value::String(s) => {
            let prefix = prefix.trim_matches('"');
            Ok(Value::Bool(s.starts_with(prefix)))
        }
        _ => Err(QueryError::TypeError("startswith() requires a string".to_string())),
    }
}

fn execute_endswith(data: &Value, suffix: &str) -> Result<Value, QueryError> {
    match data {
        Value::String(s) => {
            let suffix = suffix.trim_matches('"');
            Ok(Value::Bool(s.ends_with(suffix)))
        }
        _ => Err(QueryError::TypeError("endswith() requires a string".to_string())),
    }
}

fn execute_split(data: &Value, delimiter: &str) -> Result<Value, QueryError> {
    match data {
        Value::String(s) => {
            let delimiter = delimiter.trim_matches('"');
            let parts: Vec<Value> = s.split(delimiter)
                .map(|p| Value::String(p.to_string()))
                .collect();
            Ok(Value::Array(parts))
        }
        _ => Err(QueryError::TypeError("split() requires a string".to_string())),
    }
}

fn execute_join(data: &Value, delimiter: &str) -> Result<Value, QueryError> {
    match data {
        Value::Array(arr) => {
            let delimiter = delimiter.trim_matches('"');
            let strings: Vec<String> = arr.iter()
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    Value::Null => "null".to_string(),
                    _ => serde_json::to_string(v).unwrap_or_else(|_| "".to_string()),
                })
                .collect();
            Ok(Value::String(strings.join(delimiter)))
        }
        _ => Err(QueryError::TypeError("join() requires an array".to_string())),
    }
}

fn execute_test(data: &Value, pattern: &str) -> Result<Value, QueryError> {
    match data {
        Value::String(s) => {
            let pattern = pattern.trim_matches('"');
            let re = regex::Regex::new(pattern)
                .map_err(|e| QueryError::InvalidArgument(format!("Invalid regex: {}", e)))?;
            Ok(Value::Bool(re.is_match(s)))
        }
        _ => Err(QueryError::TypeError("test() requires a string".to_string())),
    }
}

fn execute_match(data: &Value, pattern: &str) -> Result<Value, QueryError> {
    match data {
        Value::String(s) => {
            let pattern = pattern.trim_matches('"');
            let re = regex::Regex::new(pattern)
                .map_err(|e| QueryError::InvalidArgument(format!("Invalid regex: {}", e)))?;
            
            if let Some(captures) = re.captures(s) {
                let mut result = Map::new();
                result.insert("offset".to_string(), Value::Number(serde_json::Number::from(captures.get(0).unwrap().start())));
                result.insert("length".to_string(), Value::Number(serde_json::Number::from(captures.get(0).unwrap().len())));
                result.insert("string".to_string(), Value::String(captures.get(0).unwrap().as_str().to_string()));
                
                let captures_array: Vec<Value> = captures.iter()
                    .skip(1)
                    .filter_map(|m| m.map(|c| Value::String(c.as_str().to_string())))
                    .collect();
                result.insert("captures".to_string(), Value::Array(captures_array));
                
                Ok(Value::Object(result))
            } else {
                Ok(Value::Null)
            }
        }
        _ => Err(QueryError::TypeError("match() requires a string".to_string())),
    }
}

fn execute_indices(data: &Value, value_str: &str) -> Result<Value, QueryError> {
    // Parse the value to search for
    let search_value: Value = if value_str.starts_with('"') && value_str.ends_with('"') {
        // String value
        Value::String(value_str[1..value_str.len()-1].to_string())
    } else {
        // Try to parse as number or other value
        serde_json::from_str(value_str)
            .unwrap_or_else(|_| Value::String(value_str.to_string()))
    };
    
    match data {
        Value::Array(arr) => {
            let mut indices = Vec::new();
            for (i, item) in arr.iter().enumerate() {
                if item == &search_value {
                    indices.push(Value::Number(serde_json::Number::from(i)));
                }
            }
            Ok(Value::Array(indices))
        }
        Value::String(s) => {
            if let Value::String(search_str) = &search_value {
                let mut indices = Vec::new();
                let mut start = 0;
                while let Some(pos) = s[start..].find(search_str) {
                    indices.push(Value::Number(serde_json::Number::from(start + pos)));
                    start = start + pos + 1;
                }
                Ok(Value::Array(indices))
            } else {
                Err(QueryError::TypeError("indices() on string requires a string search value".to_string()))
            }
        }
        _ => Err(QueryError::TypeError("indices() requires an array or string".to_string())),
    }
}

fn execute_error(message: &str) -> Result<Value, QueryError> {
    // Extract message from quotes if present
    let message = if message.starts_with('"') && message.ends_with('"') && message.len() >= 2 {
        &message[1..message.len()-1]
    } else {
        message
    };
    Err(QueryError::ExecutionError(message.to_string()))
}

fn execute_index(data: &Value, substring: &str) -> Result<Value, QueryError> {
    match data {
        Value::String(s) => {
            let substring = substring.trim_matches('"');
            if let Some(pos) = s.find(substring) {
                Ok(Value::Number(serde_json::Number::from(pos)))
            } else {
                Ok(Value::Null)
            }
        }
        _ => Err(QueryError::TypeError("index() requires a string".to_string())),
    }
}

fn execute_rindex(data: &Value, substring: &str) -> Result<Value, QueryError> {
    match data {
        Value::String(s) => {
            let substring = substring.trim_matches('"');
            if let Some(pos) = s.rfind(substring) {
                Ok(Value::Number(serde_json::Number::from(pos)))
            } else {
                Ok(Value::Null)
            }
        }
        _ => Err(QueryError::TypeError("rindex() requires a string".to_string())),
    }
}

fn execute_ltrimstr(data: &Value, prefix: &str) -> Result<Value, QueryError> {
    match data {
        Value::String(s) => {
            let prefix = prefix.trim_matches('"');
            if let Some(stripped) = s.strip_prefix(prefix) {
                Ok(Value::String(stripped.to_string()))
            } else {
                Ok(data.clone())
            }
        }
        _ => Err(QueryError::TypeError("ltrimstr() requires a string".to_string())),
    }
}

fn execute_rtrimstr(data: &Value, suffix: &str) -> Result<Value, QueryError> {
    match data {
        Value::String(s) => {
            let suffix = suffix.trim_matches('"');
            if let Some(stripped) = s.strip_suffix(suffix) {
                Ok(Value::String(stripped.to_string()))
            } else {
                Ok(data.clone())
            }
        }
        _ => Err(QueryError::TypeError("rtrimstr() requires a string".to_string())),
    }
}

fn paths(data: &Value) -> Result<Value, QueryError> {
    fn collect_paths(value: &Value, current_path: Vec<String>) -> Vec<Vec<String>> {
        let mut result = vec![];
        
        match value {
            Value::Object(obj) => {
                // Add current path for objects (even empty ones)
                result.push(current_path.clone());
                
                for (key, val) in obj {
                    let mut new_path = current_path.clone();
                    new_path.push(key.clone());
                    result.extend(collect_paths(val, new_path));
                }
            }
            Value::Array(arr) => {
                // Add current path for arrays (even empty ones)
                result.push(current_path.clone());
                
                for (idx, val) in arr.iter().enumerate() {
                    let mut new_path = current_path.clone();
                    new_path.push(idx.to_string());
                    result.extend(collect_paths(val, new_path));
                }
            }
            _ => {
                // Add path for scalar values
                result.push(current_path);
            }
        }
        
        result
    }
    
    let paths = collect_paths(data, vec![]);
    let json_paths: Vec<Value> = paths.into_iter()
        .filter(|p| !p.is_empty()) // Skip the root path
        .map(|p| Value::Array(p.into_iter().map(Value::String).collect()))
        .collect();
    
    Ok(Value::Array(json_paths))
}

fn leaf_paths(data: &Value) -> Result<Value, QueryError> {
    fn collect_leaf_paths(value: &Value, current_path: Vec<String>) -> Vec<Vec<String>> {
        let mut result = vec![];
        
        match value {
            Value::Object(obj) => {
                if obj.is_empty() {
                    // Empty object is a leaf
                    result.push(current_path);
                } else {
                    for (key, val) in obj {
                        let mut new_path = current_path.clone();
                        new_path.push(key.clone());
                        result.extend(collect_leaf_paths(val, new_path));
                    }
                }
            }
            Value::Array(arr) => {
                if arr.is_empty() {
                    // Empty array is a leaf
                    result.push(current_path);
                } else {
                    for (idx, val) in arr.iter().enumerate() {
                        let mut new_path = current_path.clone();
                        new_path.push(idx.to_string());
                        result.extend(collect_leaf_paths(val, new_path));
                    }
                }
            }
            _ => {
                // Scalar values are always leaves
                result.push(current_path);
            }
        }
        
        result
    }
    
    let paths = collect_leaf_paths(data, vec![]);
    let json_paths: Vec<Value> = paths.into_iter()
        .filter(|p| !p.is_empty()) // Skip the root if it's a leaf
        .map(|p| Value::Array(p.into_iter().map(Value::String).collect()))
        .collect();
    
    Ok(Value::Array(json_paths))
}

// Helper functions

fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::Number(n) => n.as_i64() != Some(0) && n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

fn compare_json_values(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (a, b) {
        (Value::Null, Value::Null) => std::cmp::Ordering::Equal,
        (Value::Null, _) => std::cmp::Ordering::Less,
        (_, Value::Null) => std::cmp::Ordering::Greater,
        (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
        (Value::Number(a), Value::Number(b)) => {
            let a_f = a.as_f64().unwrap_or(0.0);
            let b_f = b.as_f64().unwrap_or(0.0);
            a_f.partial_cmp(&b_f).unwrap_or(std::cmp::Ordering::Equal)
        }
        (Value::String(a), Value::String(b)) => a.cmp(b),
        (Value::Array(a), Value::Array(b)) => a.len().cmp(&b.len()),
        (Value::Object(a), Value::Object(b)) => a.len().cmp(&b.len()),
        _ => std::cmp::Ordering::Equal,
    }
}

fn objects(data: &Value) -> Result<Value, QueryError> {
    // In jq, 'objects' acts as a filter:
    // - For objects, it returns the object
    // - For arrays, it filters to only objects
    // - For other types, it returns empty (no output)
    match data {
        Value::Array(arr) => {
            let objects: Vec<Value> = arr.iter()
                .filter(|v| matches!(v, Value::Object(_)))
                .cloned()
                .collect();
            Ok(Value::Array(objects))
        }
        Value::Object(_) => Ok(data.clone()),
        _ => Ok(Value::Array(vec![])), // Return empty array for non-objects
    }
}