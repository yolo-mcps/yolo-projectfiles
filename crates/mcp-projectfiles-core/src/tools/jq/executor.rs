use super::{JsonQueryError, parser, functions, operators, conditionals};
use serde_json;
use jsonpath_rust::JsonPathQuery;

pub struct JsonQueryExecutor;

impl JsonQueryExecutor {
    pub fn new() -> Self {
        JsonQueryExecutor
    }
    
    pub fn execute_query(&self, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, JsonQueryError> {
        let query = query.trim();



        
        // Check for if-then-else conditionals
        if query.starts_with("if ") {

            return conditionals::execute_if_then_else(self, data, query);
        }
        
        // Check for try-catch error handling
        if query.starts_with("try ") {
            return self.execute_try_catch(data, query);
        }
        
        // Check if this is a pipe operation (but not inside parentheses)
        if parser::contains_pipe_outside_parens(query) {

            return self.execute_pipe_query(data, query);
        }
        
        // Check for alternative operator (//) before other operations
        if query.contains(" // ") {
            return self.execute_alternative_operator(data, query);
        }
        
        // Check for optional operator (?) for safe field access
        if query.contains('?') && !query.contains("?:") {
            return self.execute_optional_access(data, query);
        }
        
        // Check for with_entries FIRST (needs executor access)
        if query.starts_with("with_entries(") && query.ends_with(')') {

            return self.execute_with_entries(data, query);
        }
        
        // Check for del operation
        if query.starts_with("del(") && query.ends_with(')') {
            return functions::execute_del_operation(self, data, query);
        }
        
        // Check for array operations BEFORE arithmetic (map/select can contain arithmetic)
        if query.contains("map(") || query.contains("select(") || query.contains("[]") ||
           query == "sort" || query.starts_with("sort_by(") || query.starts_with("group_by(") ||
           (query.starts_with('[') && query.ends_with(']') && query.contains(':')) {

            return functions::execute_array_operation(self, data, query);
        }
        
        // Check for arithmetic expressions
        if parser::is_arithmetic_expression(query) {

            return operators::execute_arithmetic(self, data, query);
        }
        
        // Check for object construction
        if query.starts_with('{') && query.ends_with('}') {

            return self.execute_object_construction(data, query);
        }
        
        // Check for string functions
        // Note: Don't use contains() for function names as it can match unintended strings
        if query.starts_with("split(") || query.starts_with(".split(") ||
           query.starts_with("join(") || query.starts_with(".join(") ||
           query == "trim" || query == ".trim" ||
           query.starts_with("contains(") || query.starts_with(".contains(") ||
           query.starts_with("startswith(") || query.starts_with(".startswith(") ||
           query.starts_with("endswith(") || query.starts_with(".endswith(") ||
           query.starts_with("test(") || query.starts_with(".test(") ||
           query.starts_with("match(") || query.starts_with(".match(") ||
           query.starts_with("ltrimstr(") || query.starts_with(".ltrimstr(") ||
           query.starts_with("rtrimstr(") || query.starts_with(".rtrimstr(") ||
           query == "tostring" || query == ".tostring" ||
           query == "tonumber" || query == ".tonumber" ||
           query == "ascii_downcase" || query == ".ascii_downcase" ||
           query == "ascii_upcase" || query == ".ascii_upcase" ||
           query.contains(" | split(") || query.contains(" | join(") ||
           query.contains(" | contains(") || query.contains(" | startswith(") ||
           query.contains(" | endswith(") || query.contains(" | test(") ||
           query.contains(" | match(") || query.contains(" | ltrimstr(") ||
           query.contains(" | rtrimstr(") || query.ends_with(" | trim") ||
           query.ends_with(" | tostring") || query.ends_with(" | tonumber") ||
           query.ends_with(" | ascii_downcase") || query.ends_with(" | ascii_upcase") {

            return functions::execute_string_function(self, data, query);
        }
        
        // Check for built-in functions
        if query == "keys" || query == "values" || query == "length" || query == "type" ||
           query == ".keys" || query == ".values" || query == ".length" || query == ".type" ||
           query == "to_entries" || query == ".to_entries" || query == "from_entries" || query == ".from_entries" ||
           query == "add" || query == "min" || query == "max" || query == "unique" || 
           query == "reverse" || query == "sort" || query.starts_with("sort_by(") ||
           query == "flatten" || query.starts_with("flatten(") ||
           query.starts_with("indices(") || query.starts_with("has(") ||
           query == "paths" || query == "leaf_paths" ||
           query == "floor" || query == ".floor" ||
           query == "ceil" || query == ".ceil" ||
           query == "round" || query == ".round" ||
           query == "abs" || query == ".abs" ||
           query == "empty" || query == ".empty" ||
           query.starts_with("error(") ||
           query.ends_with(" | keys") || query.ends_with(" | values") || 
           query.ends_with(" | length") || query.ends_with(" | type") ||
           query.ends_with(" | to_entries") || query.ends_with(" | from_entries") ||
           query.ends_with(" | add") || query.ends_with(" | min") || 
           query.ends_with(" | max") || query.ends_with(" | unique") ||
           query.ends_with(" | reverse") || query.ends_with(" | sort") ||
           query.ends_with(" | flatten") ||
           query.ends_with(" | floor") || query.ends_with(" | ceil") ||
           query.ends_with(" | round") || query.ends_with(" | abs") {
            return functions::execute_builtin_function(data, query);
        }
        
        // Try JSONPath first for complex queries
        if query.starts_with('$') || query.contains('*') || query.contains("..") {
            return self.execute_jsonpath_query(data, query);
        }
        
        // Fall back to simple path query for basic operations

        self.simple_path_query(data, query)
    }
    
    fn execute_pipe_query(&self, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, JsonQueryError> {
        let parts: Vec<&str> = query.split('|').collect();
        let mut result = data.clone();
        
        for part in parts {
            let part = part.trim();
            result = self.execute_query(&result, part)?;
        }
        
        Ok(result)
    }
    
    fn simple_path_query(&self, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, JsonQueryError> {
        let query = query.trim();
        
        // Handle root reference
        if query == "." {
            return Ok(data.clone());
        }
        
        if !query.starts_with('.') {

            return Err(JsonQueryError::InvalidQuery(
                format!("Query must start with '.': {}", query)
            ));
        }
        
        let path = &query[1..]; // Remove leading '.'
        if path.is_empty() {
            return Ok(data.clone());
        }
        
        // Parse the full path with array access support
        self.parse_complex_path(data, path)
    }
    
    pub fn parse_complex_path(&self, data: &serde_json::Value, path: &str) -> Result<serde_json::Value, JsonQueryError> {
        let mut current = data.clone();
        let mut i = 0;
        let chars: Vec<char> = path.chars().collect();
        
        while i < chars.len() {
            // Parse field name
            let mut field_end = i;
            while field_end < chars.len() && chars[field_end] != '.' && chars[field_end] != '[' {
                field_end += 1;
            }
            
            if field_end > i {
                let field_name: String = chars[i..field_end].iter().collect();
                if let serde_json::Value::Object(obj) = &current {
                    current = obj.get(&field_name).unwrap_or(&serde_json::Value::Null).clone();
                } else {
                    return Ok(serde_json::Value::Null);
                }
                i = field_end;
            }
            
            // Handle array access
            while i < chars.len() && chars[i] == '[' {
                // Find the closing bracket
                let mut bracket_end = i + 1;
                while bracket_end < chars.len() && chars[bracket_end] != ']' {
                    bracket_end += 1;
                }
                
                if bracket_end >= chars.len() {
                    return Err(JsonQueryError::InvalidQuery("Missing closing bracket ]".to_string()));
                }
                
                let index_str: String = chars[i + 1..bracket_end].iter().collect();
                
                // Check if this is a slice [start:end]
                if index_str.contains(':') {
                    let parts: Vec<&str> = index_str.split(':').collect();
                    if parts.len() != 2 {
                        return Err(JsonQueryError::InvalidQuery("Invalid slice syntax".to_string()));
                    }
                    
                    if let serde_json::Value::Array(arr) = &current {
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
                        current = serde_json::Value::Array(slice);
                    } else {
                        return Ok(serde_json::Value::Null);
                    }
                } else {
                    // Regular array index
                    let index: usize = index_str.parse()
                        .map_err(|_| JsonQueryError::InvalidQuery(format!("Invalid array index: {}", index_str)))?;
                    
                    if let serde_json::Value::Array(arr) = &current {
                        if index < arr.len() {
                            current = arr[index].clone();
                        } else {
                            return Ok(serde_json::Value::Null);
                        }
                    } else {
                        return Ok(serde_json::Value::Null);
                    }
                }
                
                i = bracket_end + 1;
            }
            
            // Skip dot separator
            if i < chars.len() && chars[i] == '.' {
                i += 1;
            }
        }
        
        Ok(current)
    }
    
    fn execute_jsonpath_query(&self, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, JsonQueryError> {
        // Handle recursive descent
        if query.starts_with("..") {
            return self.handle_recursive_descent(data, query);
        }
        
        // Handle wildcard queries
        if query.contains('*') {
            return self.handle_wildcard_query(data, query);
        }
        
        // Convert jq-style path to JSONPath
        let jsonpath_query = if query.starts_with('$') {
            query.to_string()
        } else {
            format!("${}", query)
        };
        
        // Use jsonpath_rust for complex queries
        match data.clone().path(&jsonpath_query) {
            Ok(result) => {
                // If we got exactly one result, return it directly
                // Otherwise return as array
                match result {
                    serde_json::Value::Array(arr) if arr.len() == 1 => Ok(arr[0].clone()),
                    _ => Ok(result)
                }
            }
            Err(e) => Err(JsonQueryError::InvalidQuery(format!("JSONPath query failed: {}", e)))
        }
    }
    
    fn handle_recursive_descent(&self, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, JsonQueryError> {
        if query == ".." {
            // Return all values recursively
            let mut results = Vec::new();
            self.collect_all_values(data, &mut results);
            return Ok(serde_json::Value::Array(results));
        }
        
        // Extract the field name after ..
        let field_name = &query[2..];
        if field_name.is_empty() {
            return Err(JsonQueryError::InvalidQuery("Recursive descent requires a field name after ..".to_string()));
        }
        
        let mut results = Vec::new();
        self.find_field_recursive(data, field_name, &mut results);
        Ok(serde_json::Value::Array(results))
    }
    
    fn collect_all_values(&self, data: &serde_json::Value, results: &mut Vec<serde_json::Value>) {
        match data {
            serde_json::Value::Object(map) => {
                for value in map.values() {
                    results.push(value.clone());
                    self.collect_all_values(value, results);
                }
            }
            serde_json::Value::Array(arr) => {
                for value in arr {
                    results.push(value.clone());
                    self.collect_all_values(value, results);
                }
            }
            _ => {
                // Leaf values are already added by their containers
            }
        }
    }
    
    fn find_field_recursive(&self, data: &serde_json::Value, field_name: &str, results: &mut Vec<serde_json::Value>) {
        match data {
            serde_json::Value::Object(map) => {
                // Check if this object has the field
                if let Some(value) = map.get(field_name) {
                    results.push(value.clone());
                }
                
                // Recurse into all values
                for value in map.values() {
                    self.find_field_recursive(value, field_name, results);
                }
            }
            serde_json::Value::Array(arr) => {
                // Recurse into array elements
                for value in arr {
                    self.find_field_recursive(value, field_name, results);
                }
            }
            _ => {
                // Leaf nodes don't contain fields
            }
        }
    }
    
    fn handle_wildcard_query(&self, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, JsonQueryError> {
        let query = query.trim();
        
        // Simple wildcard patterns
        if query.ends_with(".*") {
            // Get all values from an object
            let prefix = &query[..query.len() - 2];
            let target = if prefix.is_empty() {
                data
            } else {
                &self.execute_query(data, prefix)?
            };
            
            match target {
                serde_json::Value::Object(map) => {
                    let values: Vec<serde_json::Value> = map.values().cloned().collect();
                    Ok(serde_json::Value::Array(values))
                }
                _ => Err(JsonQueryError::ExecutionError("Wildcard .* can only be applied to objects".to_string()))
            }
        } else if query.contains("[*]") {
            // Array wildcard
            let parts: Vec<&str> = query.split("[*]").collect();
            if parts.len() == 2 {
                let array_path = parts[0];
                let after_wildcard = parts[1];
                
                let array_value = self.execute_query(data, array_path)?;
                
                if let serde_json::Value::Array(arr) = array_value {
                    let mut results = Vec::new();
                    for item in arr {
                        if after_wildcard.is_empty() {
                            results.push(item);
                        } else {

                            // after_wildcard already includes the dot if needed
                            let query_path = if after_wildcard.starts_with('.') {
                                after_wildcard.to_string()
                            } else {
                                format!(".{}", after_wildcard)
                            };
                            let item_result = self.execute_query(&item, &query_path)?;

                            results.push(item_result);
                        }
                    }
                    Ok(serde_json::Value::Array(results))
                } else {
                    Err(JsonQueryError::ExecutionError("Array wildcard [*] can only be applied to arrays".to_string()))
                }
            } else {
                Err(JsonQueryError::InvalidQuery("Invalid wildcard pattern".to_string()))
            }
        } else {
            Err(JsonQueryError::InvalidQuery(format!("Unsupported wildcard pattern: {}", query)))
        }
    }
    
    fn execute_object_construction(&self, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, JsonQueryError> {
        // Remove braces and parse key-value pairs
        let content = query[1..query.len()-1].trim();
        let mut object = serde_json::Map::new();
        
        // More sophisticated parsing that handles nested structures
        let mut pairs = Vec::new();
        let mut current_pair = String::new();
        let mut brace_depth = 0;
        let mut in_quotes = false;
        let mut escape_next = false;
        
        for ch in content.chars() {
            if escape_next {
                current_pair.push(ch);
                escape_next = false;
                continue;
            }
            
            match ch {
                '\\' if in_quotes => {
                    escape_next = true;
                    current_pair.push(ch);
                }
                '"' => {
                    in_quotes = !in_quotes;
                    current_pair.push(ch);
                }
                '{' | '[' if !in_quotes => {
                    brace_depth += 1;
                    current_pair.push(ch);
                }
                '}' | ']' if !in_quotes => {
                    brace_depth -= 1;
                    current_pair.push(ch);
                }
                ',' if !in_quotes && brace_depth == 0 => {
                    pairs.push(current_pair.trim().to_string());
                    current_pair.clear();
                }
                _ => {
                    current_pair.push(ch);
                }
            }
        }
        
        if !current_pair.trim().is_empty() {
            pairs.push(current_pair.trim().to_string());
        }
        
        for pair in pairs {
            if let Some(colon_pos) = pair.find(':') {
                let key_part = pair[..colon_pos].trim();
                let value_part = pair[colon_pos + 1..].trim();
                
                // Extract key (remove quotes if present)
                let key = if key_part.starts_with('"') && key_part.ends_with('"') {
                    key_part[1..key_part.len()-1].to_string()
                } else {
                    key_part.to_string()
                };
                
                // Evaluate value expression
                let value = if value_part.starts_with('.') {
                    self.execute_query(data, value_part)?
                } else if value_part == "true" || value_part == "false" || value_part == "null" || 
                         value_part.parse::<f64>().is_ok() || 
                         (value_part.starts_with('"') && value_part.ends_with('"')) {
                    parser::parse_value(value_part)?
                } else {
                    // It might be a complex expression
                    self.execute_query(data, value_part)?
                };
                
                object.insert(key, value);
            }
        }
        
        Ok(serde_json::Value::Object(object))
    }
    
    fn execute_alternative_operator(&self, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, JsonQueryError> {
        // Find the rightmost // operator (to handle left-associativity)
        let mut alt_pos = None;
        let mut i = 0;
        let chars: Vec<char> = query.chars().collect();
        
        while i < chars.len() - 1 {
            if chars[i] == '/' && chars[i + 1] == '/' {
                // Check it's not in a string
                if !self.is_in_string(&chars, i) {
                    alt_pos = Some(i);
                }
                i += 2; // Skip both slashes
            } else {
                i += 1;
            }
        }
        
        if let Some(pos) = alt_pos {
            let left = query[..pos].trim();
            let right = query[pos + 2..].trim(); // Skip "//"
            
            // Evaluate the left side
            match self.execute_query(data, left) {
                Ok(value) => {
                    // Return left value if it's not null or false
                    if !value.is_null() && value != serde_json::Value::Bool(false) {
                        Ok(value)
                    } else {
                        // Otherwise evaluate and return the right side
                        self.evaluate_expression(data, right)
                    }
                }
                Err(_) => {
                    // If left side errors, return right side
                    self.evaluate_expression(data, right)
                }
            }
        } else {
            Err(JsonQueryError::InvalidQuery("No // operator found".to_string()))
        }
    }
    
    fn execute_try_catch(&self, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, JsonQueryError> {
        // Parse try-catch syntax: try EXPR [catch EXPR]
        let query = query.trim();
        
        if !query.starts_with("try ") {
            return Err(JsonQueryError::InvalidQuery("try-catch must start with 'try'".to_string()));
        }
        
        // Find the catch keyword if present
        let catch_pos = query.find(" catch ");
        
        let (try_expr, catch_expr) = if let Some(pos) = catch_pos {
            let try_expr = query[4..pos].trim(); // Skip "try "
            let catch_expr = query[pos + 7..].trim(); // Skip " catch "
            (try_expr, Some(catch_expr))
        } else {
            let try_expr = query[4..].trim(); // Skip "try "
            (try_expr, None)
        };
        
        // Try to execute the try expression
        match self.execute_query(data, try_expr) {
            Ok(value) => {
                // In jq, accessing a missing field returns null, not an error
                // So we only use catch if there's an actual error
                Ok(value)
            },
            Err(_) => {
                if let Some(catch_expr) = catch_expr {
                    // If there's a catch expression, execute it
                    // In real jq, the error would be available as input to catch
                    // For simplicity, we'll just execute the catch expression with the original data
                    self.evaluate_expression(data, catch_expr)
                } else {
                    // No catch expression, return null
                    Ok(serde_json::Value::Null)
                }
            }
        }
    }
    
    fn execute_optional_access(&self, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, JsonQueryError> {
        // Handle .field? syntax - return null instead of error if field doesn't exist
        let query = query.trim();
        
        if query.ends_with('?') {
            let path = &query[..query.len()-1]; // Remove the ?
            
            // Try to execute the query, return null on error
            match self.execute_query(data, path) {
                Ok(value) => Ok(value),
                Err(_) => Ok(serde_json::Value::Null),
            }
        } else if query.contains(".?") {
            // Handle chained optional access like .foo.?bar
            let parts: Vec<&str> = query.split(".?").collect();
            if parts.len() == 2 {
                let first_part = parts[0];
                let second_part = format!(".{}", parts[1]);
                
                // Execute the first part
                match self.execute_query(data, first_part) {
                    Ok(intermediate) => {
                        // Try to access the second part, return null on error
                        match self.execute_query(&intermediate, &second_part) {
                            Ok(value) => Ok(value),
                            Err(_) => Ok(serde_json::Value::Null),
                        }
                    }
                    Err(e) => Err(e),
                }
            } else {
                Err(JsonQueryError::InvalidQuery("Invalid optional access syntax".to_string()))
            }
        } else {
            Err(JsonQueryError::InvalidQuery("Invalid optional access syntax".to_string()))
        }
    }
    
    fn execute_with_entries(&self, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, JsonQueryError> {
        let expr = &query[13..query.len()-1];
        
        match data {
            serde_json::Value::Object(map) => {
                // Convert to entries format
                let entries: Vec<serde_json::Value> = map.iter().map(|(k, v)| {
                    serde_json::json!({
                        "key": k,
                        "value": v
                    })
                }).collect();
                
                let entries_array = serde_json::Value::Array(entries);
                
                // Apply the expression to each entry
                let mut transformed_entries = Vec::new();
                if let serde_json::Value::Array(arr) = &entries_array {
                    for entry in arr {
                        // Special handling for assignments within with_entries
                        let result = if expr.contains('=') && !expr.contains("==") {
                            // This is likely an assignment
                            let mut entry_copy = entry.clone();
                            if let Some(eq_pos) = expr.find('=') {
                                let path = expr[..eq_pos].trim();
                                let value_expr = expr[eq_pos + 1..].trim();
                                
                                // Evaluate the value expression in the context of the entry
                                let value = self.execute_query(&entry_copy, value_expr)?;
                                
                                // Set the value at the path
                                self.set_path(&mut entry_copy, path, value)?;
                                entry_copy
                            } else {
                                self.execute_query(entry, expr)?
                            }
                        } else {
                            self.execute_query(entry, expr)?
                        };
                        transformed_entries.push(result);
                    }
                }
                
                // Convert back to object
                let mut result_map = serde_json::Map::new();
                for entry in transformed_entries {
                    if let serde_json::Value::Object(obj) = entry {
                        if let (Some(key), Some(value)) = (obj.get("key"), obj.get("value")) {
                            if let serde_json::Value::String(k) = key {
                                result_map.insert(k.clone(), value.clone());
                            }
                        }
                    }
                }
                
                Ok(serde_json::Value::Object(result_map))
            }
            _ => Err(JsonQueryError::ExecutionError(
                "with_entries can only be applied to objects".to_string()
            ))
        }
    }
    
    fn evaluate_expression(&self, data: &serde_json::Value, expr: &str) -> Result<serde_json::Value, JsonQueryError> {
        let expr = expr.trim();
        
        // Check if it's a literal value
        if let Ok(value) = parser::parse_value(expr) {
            // If it parses as a value and doesn't start with '.', it's a literal
            if !expr.starts_with('.') {
                return Ok(value);
            }
        }
        
        // Otherwise, execute as a query
        self.execute_query(data, expr)
    }
    
    fn is_in_string(&self, chars: &[char], pos: usize) -> bool {
        let mut in_string = false;
        let mut escape_next = false;
        
        for i in 0..pos {
            if escape_next {
                escape_next = false;
                continue;
            }
            
            match chars[i] {
                '\\' if in_string => escape_next = true,
                '"' => in_string = !in_string,
                _ => {}
            }
        }
        
        in_string
    }
    
    pub fn set_path(&self, data: &mut serde_json::Value, path: &str, value: serde_json::Value) -> Result<(), JsonQueryError> {
        // This is a placeholder - the actual implementation would be more complex
        // For now, handle simple paths
        if !path.starts_with('.') {
            return Err(JsonQueryError::InvalidQuery("Path must start with '.'".to_string()));
        }
        
        let path = &path[1..]; // Remove leading dot
        
        // Handle simple field assignment
        if !path.contains('.') && !path.contains('[') {
            if let serde_json::Value::Object(map) = data {
                map.insert(path.to_string(), value);
                return Ok(());
            } else {
                return Err(JsonQueryError::ExecutionError("Cannot set field on non-object".to_string()));
            }
        }
        
        // For complex paths, we'd need to traverse and create intermediate objects/arrays as needed
        // This is a simplified version
        Err(JsonQueryError::ExecutionError("Complex path assignment not yet implemented".to_string()))
    }
}