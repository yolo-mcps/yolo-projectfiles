use serde_json::{Value, Map, json};
use super::errors::QueryError;
use super::executor::QueryEngine;

/// Execute a conditional expression
pub fn execute_conditional(engine: &QueryEngine, data: &Value, query: &str) -> Result<Value, QueryError> {
    let conditional = engine.parser.parse_conditional(query)?;
    
    // Evaluate condition
    let condition_result = engine.execute(data, &conditional.condition)?;
    let is_true = match &condition_result {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::Number(n) => n.as_i64() != Some(0) && n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    };
    
    if is_true {
        engine.execute(data, &conditional.then_expr)
    } else if let Some(else_expr) = &conditional.else_expr {
        engine.execute(data, else_expr)
    } else {
        Ok(Value::Null)
    }
}

/// Execute a try-catch expression
pub fn execute_try_catch(engine: &QueryEngine, data: &Value, query: &str) -> Result<Value, QueryError> {
    if !query.starts_with("try ") {
        return Err(QueryError::InvalidSyntax("Try expression must start with 'try'".to_string()));
    }
    
    let catch_pos = query.find(" catch ");
    if let Some(pos) = catch_pos {
        let try_expr = query[4..pos].trim();
        let catch_expr = query[pos + 7..].trim();
        
        match engine.execute(data, try_expr) {
            Ok(result) => Ok(result),
            Err(_) => {
                // Parse catch expression - if it's a literal, return it
                engine.parser.parse_value(catch_expr)
                    .or_else(|_| engine.execute(data, catch_expr))
            }
        }
    } else {
        // Try without catch - return null on error
        let try_expr = query[4..].trim();
        match engine.execute(data, try_expr) {
            Ok(result) => Ok(result),
            Err(_) => Ok(Value::Null),
        }
    }
}

/// Execute a pipe expression
pub fn execute_pipe(engine: &QueryEngine, data: &Value, query: &str) -> Result<Value, QueryError> {
    let parts = engine.parser.parse_pipe_expression(query);
    let mut result = data.clone();
    
    for (i, part) in parts.iter().enumerate() {
        // Check if this is an array iterator followed by further operations
        if part.ends_with("[]") && i + 1 < parts.len() {
            // Execute the array iterator part
            let iter_result = engine.execute(&result, part)?;
            if let Value::Array(arr) = iter_result {
                // Apply remaining operations to each element and collect results
                let remaining_parts = &parts[i + 1..];
                let remaining_query = remaining_parts.join(" | ");
                
                let mut filtered_results = Vec::new();
                for item in arr {
                    match engine.execute(&item, &remaining_query) {
                        Ok(Value::Null) => {
                            // Skip null results (common for select operations)
                        }
                        Ok(item_result) => {
                            filtered_results.push(item_result);
                        }
                        Err(_) => {
                            // Skip failed operations (e.g., select returning empty)
                        }
                    }
                }
                return Ok(Value::Array(filtered_results));
            }
        } else {
            result = engine.execute(&result, part)?;
        }
    }
    
    Ok(result)
}

/// Execute an alternative expression (// operator)
pub fn execute_alternative(engine: &QueryEngine, data: &Value, query: &str) -> Result<Value, QueryError> {
    let parts: Vec<&str> = query.splitn(2, " // ").collect();
    if parts.len() != 2 {
        return Err(QueryError::InvalidSyntax("Invalid alternative expression".to_string()));
    }
    
    match engine.execute(data, parts[0].trim()) {
        Ok(Value::Null) | Ok(Value::Bool(false)) | Err(_) => {
            // Try the alternative - which might itself contain // operators
            let alternative = parts[1].trim();
            
            // First try to parse as a literal value
            if let Ok(value) = engine.parser.parse_value(alternative) {
                // But only use the literal if it's not a query that contains //
                if !alternative.contains(" // ") {
                    return Ok(value);
                }
            }
            
            // Otherwise execute as a query (which might contain more // operators)
            engine.execute(data, alternative)
        }
        Ok(result) => Ok(result),
    }
}

/// Execute array construction
pub fn execute_array_construction(engine: &QueryEngine, data: &Value, query: &str) -> Result<Value, QueryError> {
    let content = &query[1..query.len() - 1].trim();
    if content.is_empty() {
        return Ok(Value::Array(vec![]));
    }
    
    // Check if this is a single expression (no commas at the top level)
    if !has_top_level_comma(content) {
        // Single expression that returns an array
        let result = engine.execute(data, content)?;
        // If the result is already an array, return it; otherwise wrap it
        match result {
            Value::Array(_) => Ok(result),
            _ => Ok(Value::Array(vec![result])),
        }
    } else {
        // Multiple elements separated by commas
        let elements: Result<Vec<Value>, QueryError> = split_top_level_commas(content)
            .into_iter()
            .map(|elem| {
                let elem = elem.trim();
                engine.parser.parse_value(elem)
                    .or_else(|_| engine.execute(data, elem))
            })
            .collect();
        
        Ok(Value::Array(elements?))
    }
}

/// Execute array slicing
pub fn execute_array_slicing(_engine: &QueryEngine, data: &Value, query: &str) -> Result<Value, QueryError> {
    let content = &query[1..query.len() - 1].trim();
    
    match data {
        Value::Array(arr) => {
            // Parse slice syntax [start:end]
            let parts: Vec<&str> = content.split(':').collect();
            if parts.len() != 2 {
                return Err(QueryError::InvalidSyntax("Array slice must be in format [start:end]".to_string()));
            }
            
            let start = if parts[0].is_empty() {
                0
            } else {
                parts[0].parse::<usize>()
                    .map_err(|_| QueryError::InvalidSyntax("Invalid slice start index".to_string()))?
            };
            
            let end = if parts[1].is_empty() {
                arr.len()
            } else {
                parts[1].parse::<usize>()
                    .map_err(|_| QueryError::InvalidSyntax("Invalid slice end index".to_string()))?
            };
            
            // Clamp indices to array bounds
            let start = start.min(arr.len());
            let end = end.min(arr.len());
            
            if start <= end {
                Ok(Value::Array(arr[start..end].to_vec()))
            } else {
                Ok(Value::Array(vec![]))
            }
        }
        _ => Err(QueryError::TypeError("Array slicing requires an array".to_string())),
    }
}

/// Execute object construction
pub fn execute_object_construction(engine: &QueryEngine, data: &Value, query: &str) -> Result<Value, QueryError> {
    let content = &query[1..query.len() - 1].trim();
    if content.is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    
    let mut object = Map::new();
    
    // Parse object construction
    for pair in content.split(',') {
        let pair = pair.trim();
        if let Some(colon_pos) = pair.find(':') {
            // Explicit key:value syntax
            let key = pair[..colon_pos].trim();
            let value_str = pair[colon_pos + 1..].trim();
            
            // Remove quotes from key if present
            let key = if key.starts_with('"') && key.ends_with('"') {
                &key[1..key.len() - 1]
            } else {
                key
            };
            
            // Try to parse as literal value first, then as query if that fails
            let value = if value_str.starts_with('.') || value_str.contains(' ') || value_str.contains('(') {
                // Looks like a query, try executing first
                engine.execute(data, value_str)
                    .or_else(|_| engine.parser.parse_value(value_str))?
            } else {
                // Check if it's a known function name
                let is_function = matches!(value_str, "add" | "length" | "keys" | "values" | "type" | 
                                         "reverse" | "sort" | "unique" | "flatten" | "min" | "max" |
                                         "empty" | "not" | "to_entries" | "from_entries" | "floor" |
                                         "ceil" | "round" | "abs" | "tostring" | "tonumber" | "trim" |
                                         "ascii_upcase" | "ascii_downcase" | "paths" | "leaf_paths");
                
                if is_function {
                    // Execute as function
                    engine.execute(data, value_str)?
                } else {
                    // Try parsing as literal first, then as query
                    engine.parser.parse_value(value_str)
                        .or_else(|_| engine.execute(data, value_str))?
                }
            };
            
            object.insert(key.to_string(), value);
        } else {
            // Shorthand syntax: {name} means {name: .name}
            let key = pair;
            
            // Remove quotes from key if present
            let key = if key.starts_with('"') && key.ends_with('"') {
                &key[1..key.len() - 1]
            } else {
                key
            };
            
            // Execute path query for the key
            let path_query = format!(".{}", key);
            let value = engine.execute(data, &path_query)?;
            
            object.insert(key.to_string(), value);
        }
    }
    
    Ok(Value::Object(object))
}

/// Execute an arithmetic or logical expression
pub fn execute_expression(engine: &QueryEngine, data: &Value, query: &str) -> Result<Value, QueryError> {
    let query = query.trim();
    
    // Handle NOT operator
    if query.starts_with("not ") {
        let expr = &query[4..];
        let result = engine.execute(data, expr)?;
        return Ok(Value::Bool(!is_truthy(&result)));
    }
    
    // Handle parentheses first
    if query.starts_with('(') && query.ends_with(')') {
        let inner = &query[1..query.len()-1];
        return engine.execute(data, inner);
    }
    
    // Handle binary operators (respecting parentheses)
    // Order matters for precedence - check lower precedence operators first
    // When parsing, we want to split on the lowest precedence operator first
    let operators = [
        " or ",                    // Logical OR (lowest precedence)
        " and ",                   // Logical AND
        " == ", " != ", " < ", " > ", " <= ", " >= ",  // Comparison operators
        " + ", " - ",              // Addition/subtraction
        " * ", " / ", " % "        // Multiplication/division (highest precedence)
    ];
    
    // Process operators in order (lowest to highest precedence)
    for &op in operators.iter() {
        // Find operator not inside parentheses
        let mut paren_depth = 0;
        let mut in_string = false;
        let mut escape_next = false;
        
        for (i, ch) in query.char_indices() {
            if escape_next {
                escape_next = false;
                continue;
            }
            
            match ch {
                '\\' if in_string => escape_next = true,
                '"' => in_string = !in_string,
                '(' if !in_string => paren_depth += 1,
                ')' if !in_string => paren_depth -= 1,
                _ => {
                    if paren_depth == 0 && !in_string && query[i..].starts_with(op) {
                        let left_expr = query[..i].trim();
                        let right_expr = query[i + op.len()..].trim();
                        
                        // Parse left and right expressions
                        // For function names like "add", "length", etc., we should execute them on the data
                        // Check if it's a known function or contains operators
                        let is_function_or_complex = |expr: &str| {
                            expr.starts_with('.') || 
                            expr.contains('[') || 
                            expr.contains('(') || 
                            operators.iter().any(|&o| expr.contains(o)) ||
                            // Check for common functions without parentheses
                            matches!(expr, "add" | "length" | "keys" | "values" | "type" | 
                                    "reverse" | "sort" | "unique" | "flatten" | "min" | "max" |
                                    "empty" | "not" | "to_entries" | "from_entries" | "floor" |
                                    "ceil" | "round" | "abs" | "tostring" | "tonumber" | "trim" |
                                    "ascii_upcase" | "ascii_downcase" | "paths" | "leaf_paths")
                        };
                        
                        let left = if is_function_or_complex(left_expr) {
                            engine.execute(data, left_expr)?
                        } else {
                            engine.parser.parse_value(left_expr)
                                .or_else(|_| engine.execute(data, left_expr))?
                        };
                        
                        let right = if is_function_or_complex(right_expr) {
                            engine.execute(data, right_expr)?
                        } else {
                            engine.parser.parse_value(right_expr)
                                .or_else(|_| engine.execute(data, right_expr))?
                        };
                        
                        return match op {
                            " and " => Ok(Value::Bool(is_truthy(&left) && is_truthy(&right))),
                            " or " => Ok(Value::Bool(is_truthy(&left) || is_truthy(&right))),
                            " == " => Ok(Value::Bool(left == right)),
                            " != " => Ok(Value::Bool(left != right)),
                            " < " => compare_values(&left, &right, |a, b| a < b),
                            " > " => compare_values(&left, &right, |a, b| a > b),
                            " <= " => compare_values(&left, &right, |a, b| a <= b),
                            " >= " => compare_values(&left, &right, |a, b| a >= b),
                            " + " => add_values(&left, &right),
                            " - " => subtract_values(&left, &right),
                            " * " => multiply_values(&left, &right),
                            " / " => divide_values(&left, &right),
                            " % " => modulo_values(&left, &right),
                            _ => unreachable!(),
                        };
                    }
                }
            }
        }
    }
    
    Err(QueryError::InvalidSyntax(format!("Invalid expression: {}", query)))
}

/// Execute a path query
pub fn execute_path_query(_engine: &QueryEngine, data: &Value, query: &str) -> Result<Value, QueryError> {
    let query = query.trim();
    
    // Handle root reference
    if query == "." {
        return Ok(data.clone());
    }
    
    // Handle recursive descent
    if query == ".." {
        return Ok(collect_all_values(data));
    }
    
    if query.starts_with("..") {
        let field = &query[2..];
        return Ok(collect_recursive_field(data, field));
    }
    
    // Handle array iterator
    if query == ".[]" {
        return match data {
            Value::Array(arr) => Ok(Value::Array(arr.clone())),
            Value::Object(obj) => Ok(Value::Array(obj.values().cloned().collect())),
            _ => Err(QueryError::TypeError("Cannot iterate over non-array/object".to_string())),
        };
    }
    
    // Regular path query
    if !query.starts_with('.') {
        return Err(QueryError::InvalidSyntax("Path must start with '.'".to_string()));
    }
    
    let path = &query[1..];
    navigate_path(data, path)
}

/// Execute with_entries operation
pub fn with_entries(engine: &QueryEngine, data: &Value, query: &str) -> Result<Value, QueryError> {
    // Parse with_entries(expr) syntax
    if !query.starts_with("with_entries(") || !query.ends_with(')') {
        return Err(QueryError::InvalidSyntax("with_entries must be in format with_entries(expr)".to_string()));
    }
    
    let expr = &query[13..query.len()-1].trim();
    
    match data {
        Value::Object(obj) => {
            // Convert to entries format
            let entries: Vec<Value> = obj.iter().map(|(k, v)| {
                json!({
                    "key": k,
                    "value": v
                })
            }).collect();
            
            // Apply expression to each entry
            let mut new_entries = Vec::new();
            for entry in entries {
                // Check if this is an assignment operation
                if expr.contains(" = ") {
                    // Handle assignment within with_entries
                    let mut entry_copy = entry.clone();
                    
                    // Split the assignment
                    let parts: Vec<&str> = expr.splitn(2, " = ").collect();
                    if parts.len() == 2 {
                        let target_path = parts[0].trim();
                        let value_expr = parts[1].trim();
                        
                        // Evaluate the right-hand side expression
                        let new_value = engine.execute(&entry, value_expr)?;
                        
                        // Apply the assignment to the entry
                        if let Value::Object(ref mut entry_obj) = entry_copy {
                            // Handle simple field assignment like .value = ...
                            if target_path.starts_with('.') {
                                let field_name = &target_path[1..];
                                entry_obj.insert(field_name.to_string(), new_value);
                            }
                        }
                        new_entries.push(entry_copy);
                    } else {
                        new_entries.push(entry);
                    }
                } else {
                    // Regular expression, just execute it
                    let result = engine.execute(&entry, expr)?;
                    new_entries.push(result);
                }
            }
            
            // Convert back to object
            let mut new_obj = serde_json::Map::new();
            for entry in new_entries {
                if let Value::Object(entry_obj) = entry {
                    if let (Some(Value::String(key)), Some(value)) = 
                        (entry_obj.get("key"), entry_obj.get("value")) {
                        new_obj.insert(key.clone(), value.clone());
                    }
                }
            }
            
            Ok(Value::Object(new_obj))
        }
        _ => Err(QueryError::TypeError("with_entries only works on objects".to_string())),
    }
}

/// Execute del operation
pub fn del(data: &Value, query: &str) -> Result<Value, QueryError> {
    // Parse del(path) syntax
    if !query.starts_with("del(") || !query.ends_with(')') {
        return Err(QueryError::InvalidSyntax("del must be in format del(path)".to_string()));
    }
    
    let path = &query[4..query.len()-1].trim();
    
    // Clone the data and delete the path
    let mut result = data.clone();
    delete_path(&mut result, path)?;
    Ok(result)
}

fn delete_path(data: &mut Value, path: &str) -> Result<(), QueryError> {
    if !path.starts_with('.') {
        return Err(QueryError::InvalidSyntax("Path must start with '.'".to_string()));
    }
    
    let path = &path[1..]; // Remove leading dot
    
    // Handle empty path (just ".")
    if path.is_empty() {
        return Err(QueryError::InvalidSyntax("Cannot delete root".to_string()));
    }
    
    // Split path into segments
    let segments: Vec<&str> = path.split('.').collect();
    
    // Navigate to parent of target
    let mut current = data;
    for (i, segment) in segments.iter().enumerate() {
        if i == segments.len() - 1 {
            // Last segment - delete it
            match current {
                Value::Object(obj) => {
                    obj.remove(*segment);
                    return Ok(());
                }
                Value::Array(arr) => {
                    // Try to parse as array index
                    if let Some(idx) = segment.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                        if let Ok(index) = idx.parse::<usize>() {
                            if index < arr.len() {
                                arr.remove(index);
                                return Ok(());
                            }
                        }
                    }
                    return Err(QueryError::KeyNotFound(format!("Invalid array index: {}", segment)));
                }
                _ => return Err(QueryError::TypeError("Cannot delete from non-object/array".to_string())),
            }
        } else {
            // Navigate deeper
            current = match current {
                Value::Object(obj) => {
                    obj.get_mut(*segment)
                        .ok_or_else(|| QueryError::KeyNotFound(segment.to_string()))?
                }
                Value::Array(arr) => {
                    // Handle array index
                    if let Some(idx) = segment.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                        if let Ok(index) = idx.parse::<usize>() {
                            arr.get_mut(index)
                                .ok_or_else(|| QueryError::KeyNotFound(format!("Array index out of bounds: {}", index)))?
                        } else {
                            return Err(QueryError::InvalidSyntax(format!("Invalid array index: {}", idx)));
                        }
                    } else {
                        return Err(QueryError::TypeError("Cannot access field on array".to_string()));
                    }
                }
                _ => return Err(QueryError::TypeError(format!("Cannot navigate through {}", segment))),
            };
        }
    }
    
    Ok(())
}

/// Set a value at a path
pub fn set_path(data: &mut Value, path: &str, value: Value) -> Result<(), QueryError> {
    let path = path.trim();
    
    // Handle root assignment
    if path == "." {
        *data = value;
        return Ok(());
    }
    
    if !path.starts_with('.') {
        return Err(QueryError::InvalidSyntax("Path must start with '.'".to_string()));
    }
    
    let path = &path[1..]; // Remove leading '.'
    
    // Navigate to the parent of the target
    let segments: Vec<&str> = path.split('.').collect();
    let mut current = data;
    
    for (i, segment) in segments.iter().enumerate() {
        if segment.is_empty() {
            continue;
        }
        
        let is_last = i == segments.len() - 1;
        
        // Handle array access
        if let Some(bracket_pos) = segment.find('[') {
            let field = &segment[..bracket_pos];
            
            // Navigate to field if not empty
            if !field.is_empty() {
                if is_last && bracket_pos == segment.len() - 1 {
                    // Setting a field, not an array element
                    if let Value::Object(obj) = current {
                        obj.insert(field.to_string(), value);
                        return Ok(());
                    } else {
                        return Err(QueryError::TypeError("Cannot set field on non-object".to_string()));
                    }
                }
                
                // Navigate to the field
                if let Value::Object(obj) = current {
                    if !obj.contains_key(field) {
                        obj.insert(field.to_string(), Value::Array(vec![]));
                    }
                    current = obj.get_mut(field).unwrap();
                } else {
                    return Err(QueryError::TypeError("Cannot access field on non-object".to_string()));
                }
            }
            
            // Parse array index
            let index_part = &segment[bracket_pos + 1..];
            if let Some(close_pos) = index_part.find(']') {
                let index_str = &index_part[..close_pos];
                let index: usize = index_str.parse()
                    .map_err(|_| QueryError::InvalidSyntax(format!("Invalid array index: {}", index_str)))?;
                
                if let Value::Array(arr) = current {
                    if is_last {
                        // Set the array element
                        if index < arr.len() {
                            arr[index] = value;
                            return Ok(());
                        } else {
                            return Err(QueryError::IndexOutOfBounds(format!("Index {} out of bounds", index)));
                        }
                    } else {
                        // Navigate to the array element
                        if index < arr.len() {
                            current = &mut arr[index];
                        } else {
                            return Err(QueryError::IndexOutOfBounds(format!("Index {} out of bounds", index)));
                        }
                    }
                } else {
                    return Err(QueryError::TypeError("Cannot index non-array".to_string()));
                }
            } else {
                return Err(QueryError::InvalidSyntax("Unclosed bracket".to_string()));
            }
        } else {
            // Regular field access
            if is_last {
                // Set the field
                if let Value::Object(obj) = current {
                    obj.insert(segment.to_string(), value);
                    return Ok(());
                } else {
                    return Err(QueryError::TypeError("Cannot set field on non-object".to_string()));
                }
            } else {
                // Navigate to the field
                if let Value::Object(obj) = current {
                    if !obj.contains_key(*segment) {
                        obj.insert(segment.to_string(), Value::Object(Map::new()));
                    }
                    current = obj.get_mut(*segment).unwrap();
                } else {
                    return Err(QueryError::TypeError("Cannot access field on non-object".to_string()));
                }
            }
        }
    }
    
    Ok(())
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

fn compare_values<F>(left: &Value, right: &Value, op: F) -> Result<Value, QueryError>
where
    F: Fn(f64, f64) -> bool,
{
    // Try to coerce both values to numbers for comparison
    let left_num = match left {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse::<f64>().ok(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    };
    
    let right_num = match right {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse::<f64>().ok(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    };
    
    match (left_num, right_num) {
        (Some(l), Some(r)) => Ok(Value::Bool(op(l, r))),
        _ => {
            // If numeric comparison fails, try string comparison for equality/inequality
            match (left, right) {
                (Value::String(_l), Value::String(_r)) => {
                    // For string comparison, we can only do equality/inequality reliably
                    // without knowing which operator we have
                    Err(QueryError::TypeError("String comparison only supports == and !=".to_string()))
                }
                _ => Err(QueryError::TypeError(format!("Cannot compare {} and {}", 
                    match left {
                        Value::Null => "null",
                        Value::Bool(_) => "boolean",
                        Value::Number(_) => "number",
                        Value::String(_) => "string",
                        Value::Array(_) => "array",
                        Value::Object(_) => "object",
                    },
                    match right {
                        Value::Null => "null",
                        Value::Bool(_) => "boolean",
                        Value::Number(_) => "number",
                        Value::String(_) => "string",
                        Value::Array(_) => "array",
                        Value::Object(_) => "object",
                    }
                )))
            }
        }
    }
}

fn add_values(left: &Value, right: &Value) -> Result<Value, QueryError> {
    match (left, right) {
        (Value::Number(l), Value::Number(r)) => {
            // Always use float arithmetic to match jq behavior
            let l_f = l.as_f64().ok_or_else(|| QueryError::ExecutionError("Invalid number".to_string()))?;
            let r_f = r.as_f64().ok_or_else(|| QueryError::ExecutionError("Invalid number".to_string()))?;
            let result = l_f + r_f;
            
            serde_json::Number::from_f64(result)
                .map(Value::Number)
                .ok_or_else(|| QueryError::ExecutionError("Invalid number result".to_string()))
        }
        (Value::String(l), Value::String(r)) => Ok(Value::String(format!("{}{}", l, r))),
        (Value::Array(l), Value::Array(r)) => {
            let mut result = l.clone();
            result.extend(r.iter().cloned());
            Ok(Value::Array(result))
        }
        _ => Err(QueryError::TypeError(format!("Cannot add {} and {}", 
            match left {
                Value::Null => "null",
                Value::Bool(_) => "boolean",
                Value::Number(_) => "number",
                Value::String(_) => "string",
                Value::Array(_) => "array",
                Value::Object(_) => "object",
            },
            match right {
                Value::Null => "null",
                Value::Bool(_) => "boolean",
                Value::Number(_) => "number",
                Value::String(_) => "string",
                Value::Array(_) => "array",
                Value::Object(_) => "object",
            }
        ))),
    }
}

fn subtract_values(left: &Value, right: &Value) -> Result<Value, QueryError> {
    match (left, right) {
        (Value::Number(l), Value::Number(r)) => {
            // Always use float arithmetic to match jq behavior
            let l_f = l.as_f64().ok_or_else(|| QueryError::ExecutionError("Invalid number".to_string()))?;
            let r_f = r.as_f64().ok_or_else(|| QueryError::ExecutionError("Invalid number".to_string()))?;
            let result = l_f - r_f;
            
            serde_json::Number::from_f64(result)
                .map(Value::Number)
                .ok_or_else(|| QueryError::ExecutionError("Invalid number result".to_string()))
        }
        _ => Err(QueryError::TypeError("Cannot subtract these types".to_string())),
    }
}

fn multiply_values(left: &Value, right: &Value) -> Result<Value, QueryError> {
    match (left, right) {
        (Value::Number(l), Value::Number(r)) => {
            // Always use float arithmetic to match jq behavior
            let l_f = l.as_f64().ok_or_else(|| QueryError::ExecutionError("Invalid number".to_string()))?;
            let r_f = r.as_f64().ok_or_else(|| QueryError::ExecutionError("Invalid number".to_string()))?;
            let result = l_f * r_f;
            
            serde_json::Number::from_f64(result)
                .map(Value::Number)
                .ok_or_else(|| QueryError::ExecutionError("Invalid number result".to_string()))
        }
        _ => Err(QueryError::TypeError("Cannot multiply these types".to_string())),
    }
}

fn divide_values(left: &Value, right: &Value) -> Result<Value, QueryError> {
    match (left, right) {
        (Value::Number(l), Value::Number(r)) => {
            // Always use float arithmetic to match jq behavior
            let l_f = l.as_f64().ok_or_else(|| QueryError::ExecutionError("Invalid number".to_string()))?;
            let r_f = r.as_f64().ok_or_else(|| QueryError::ExecutionError("Invalid number".to_string()))?;
            
            if r_f == 0.0 {
                return Err(QueryError::DivisionByZero);
            }
            
            let result = l_f / r_f;
            
            serde_json::Number::from_f64(result)
                .map(Value::Number)
                .ok_or_else(|| QueryError::ExecutionError("Invalid number result".to_string()))
        }
        _ => Err(QueryError::TypeError("Cannot divide these types".to_string())),
    }
}

fn modulo_values(left: &Value, right: &Value) -> Result<Value, QueryError> {
    match (left, right) {
        (Value::Number(l), Value::Number(r)) => {
            // Always use float arithmetic to match jq behavior
            let l_f = l.as_f64().ok_or_else(|| QueryError::ExecutionError("Invalid number".to_string()))?;
            let r_f = r.as_f64().ok_or_else(|| QueryError::ExecutionError("Invalid number".to_string()))?;
            
            if r_f == 0.0 {
                return Err(QueryError::DivisionByZero);
            }
            
            let result = l_f % r_f;
            
            serde_json::Number::from_f64(result)
                .map(Value::Number)
                .ok_or_else(|| QueryError::ExecutionError("Invalid number result".to_string()))
        }
        _ => Err(QueryError::TypeError("Cannot modulo these types".to_string())),
    }
}

fn collect_all_values(data: &Value) -> Value {
    let mut values = Vec::new();
    collect_values_recursive(data, &mut values);
    Value::Array(values)
}

fn collect_values_recursive(data: &Value, values: &mut Vec<Value>) {
    values.push(data.clone());
    match data {
        Value::Array(arr) => {
            for item in arr {
                collect_values_recursive(item, values);
            }
        }
        Value::Object(obj) => {
            for (_, value) in obj {
                collect_values_recursive(value, values);
            }
        }
        _ => {}
    }
}

fn collect_recursive_field(data: &Value, field: &str) -> Value {
    let mut results = Vec::new();
    collect_field_recursive(data, field, &mut results);
    Value::Array(results)
}

fn collect_field_recursive(data: &Value, field: &str, results: &mut Vec<Value>) {
    match data {
        Value::Object(obj) => {
            if let Some(value) = obj.get(field) {
                results.push(value.clone());
            }
            for (_, value) in obj {
                collect_field_recursive(value, field, results);
            }
        }
        Value::Array(arr) => {
            for item in arr {
                collect_field_recursive(item, field, results);
            }
        }
        _ => {}
    }
}

fn navigate_path(data: &Value, path: &str) -> Result<Value, QueryError> {
    navigate_path_segments(data, &split_path_segments(path))
}

fn split_path_segments(path: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut in_brackets = false;
    
    for ch in path.chars() {
        match ch {
            '.' if !in_brackets => {
                if !current.is_empty() {
                    segments.push(current.clone());
                    current.clear();
                }
            }
            '[' => {
                in_brackets = true;
                current.push(ch);
            }
            ']' => {
                in_brackets = false;
                current.push(ch);
            }
            _ => current.push(ch),
        }
    }
    
    if !current.is_empty() {
        segments.push(current);
    }
    
    segments
}

fn navigate_path_segments(data: &Value, segments: &[String]) -> Result<Value, QueryError> {
    if segments.is_empty() {
        return Ok(data.clone());
    }
    
    let segment = &segments[0];
    let remaining = &segments[1..];
    
    // Handle array access
    if let Some(bracket_pos) = segment.find('[') {
        let field = &segment[..bracket_pos];
        let mut current = data;
        
        // Navigate to field if present
        if !field.is_empty() {
            if let Value::Object(obj) = current {
                current = obj.get(field).unwrap_or(&Value::Null);
            } else {
                return Ok(Value::Null);
            }
        }
        
        // Parse array index
        let index_part = &segment[bracket_pos + 1..];
        if let Some(close_pos) = index_part.find(']') {
            let index_str = &index_part[..close_pos];
            
            if index_str == "*" || index_str.is_empty() {
                // Handle wildcard or empty brackets (both mean iterate over all elements)
                if let Value::Array(arr) = current {
                    if remaining.is_empty() {
                        // No more path segments, return the array itself
                        return Ok(Value::Array(arr.clone()));
                    } else {
                        // Apply remaining path to each element
                        let mut results = Vec::new();
                        for item in arr {
                            if let Ok(result) = navigate_path_segments(item, remaining) {
                                results.push(result);
                            }
                        }
                        return Ok(Value::Array(results));
                    }
                } else {
                    return Ok(Value::Null);
                }
            } else if index_str.contains(':') {
                // Handle array slicing
                if let Value::Array(arr) = current {
                    let parts: Vec<&str> = index_str.split(':').collect();
                    if parts.len() != 2 {
                        return Err(QueryError::InvalidSyntax("Array slice must be in format [start:end]".to_string()));
                    }
                    
                    let start = if parts[0].is_empty() {
                        0
                    } else {
                        parts[0].parse::<usize>()
                            .map_err(|_| QueryError::InvalidSyntax("Invalid slice start index".to_string()))?
                    };
                    
                    let end = if parts[1].is_empty() {
                        arr.len()
                    } else {
                        parts[1].parse::<usize>()
                            .map_err(|_| QueryError::InvalidSyntax("Invalid slice end index".to_string()))?
                    };
                    
                    // Clamp indices to array bounds
                    let start = start.min(arr.len());
                    let end = end.min(arr.len());
                    
                    if start <= end {
                        let sliced = Value::Array(arr[start..end].to_vec());
                        return navigate_path_segments(&sliced, remaining);
                    } else {
                        return Ok(Value::Array(vec![]));
                    }
                } else {
                    return Ok(Value::Null);
                }
            } else {
                // Regular array index
                let index: usize = index_str.parse()
                    .map_err(|_| QueryError::InvalidSyntax(format!("Invalid array index: {}", index_str)))?;
                
                if let Value::Array(arr) = current {
                    if let Some(item) = arr.get(index) {
                        return navigate_path_segments(item, remaining);
                    } else {
                        return Ok(Value::Null);
                    }
                } else {
                    return Ok(Value::Null);
                }
            }
        }
    } else {
        // Regular field access or object wildcard
        if segment == "*" {
            // Handle object wildcard - return all values
            match data {
                Value::Object(obj) => {
                    if remaining.is_empty() {
                        // Just return all values as an array
                        let values: Vec<Value> = obj.values().cloned().collect();
                        return Ok(Value::Array(values));
                    } else {
                        // Apply remaining path to each value
                        let mut results = Vec::new();
                        for value in obj.values() {
                            if let Ok(result) = navigate_path_segments(value, remaining) {
                                results.push(result);
                            }
                        }
                        return Ok(Value::Array(results));
                    }
                }
                Value::Array(arr) => {
                    // Also support .* on arrays (same as .[*])
                    if remaining.is_empty() {
                        return Ok(Value::Array(arr.clone()));
                    } else {
                        let mut results = Vec::new();
                        for item in arr {
                            if let Ok(result) = navigate_path_segments(item, remaining) {
                                results.push(result);
                            }
                        }
                        return Ok(Value::Array(results));
                    }
                }
                _ => return Ok(Value::Null),
            }
        } else if let Value::Object(obj) = data {
            if let Some(value) = obj.get(segment) {
                return navigate_path_segments(value, remaining);
            } else {
                return Ok(Value::Null);
            }
        } else {
            return Ok(Value::Null);
        }
    }
    
    Ok(Value::Null)
}

/// Check if a string has a comma at the top level (not inside brackets, braces, or quotes)
fn has_top_level_comma(s: &str) -> bool {
    let mut depth = 0;
    let mut in_string = false;
    let mut escape = false;
    
    for ch in s.chars() {
        if escape {
            escape = false;
            continue;
        }
        
        match ch {
            '\\' if in_string => escape = true,
            '"' => in_string = !in_string,
            '[' | '{' | '(' if !in_string => depth += 1,
            ']' | '}' | ')' if !in_string => depth -= 1,
            ',' if !in_string && depth == 0 => return true,
            _ => {}
        }
    }
    
    false
}

/// Split a string by top-level commas (not inside brackets, braces, or quotes)
fn split_top_level_commas(s: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    let mut in_string = false;
    let mut escape = false;
    
    for ch in s.chars() {
        if escape {
            escape = false;
            current.push(ch);
            continue;
        }
        
        match ch {
            '\\' if in_string => {
                escape = true;
                current.push(ch);
            }
            '"' => {
                in_string = !in_string;
                current.push(ch);
            }
            '[' | '{' | '(' if !in_string => {
                depth += 1;
                current.push(ch);
            }
            ']' | '}' | ')' if !in_string => {
                depth -= 1;
                current.push(ch);
            }
            ',' if !in_string && depth == 0 => {
                result.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    
    if !current.trim().is_empty() {
        result.push(current.trim().to_string());
    }
    
    result
}