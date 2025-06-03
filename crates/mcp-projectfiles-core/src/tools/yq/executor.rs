use super::{YamlQueryError, parser, functions, operators, conditionals};
use serde_json;

pub struct YamlQueryExecutor;

impl YamlQueryExecutor {
    pub fn new() -> Self {
        YamlQueryExecutor
    }
    
    pub fn execute(&self, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
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
        if query == "keys" || query == ".keys" || query.ends_with(" | keys") ||
           query == "values" || query == ".values" || query.ends_with(" | values") ||
           query == "length" || query == ".length" || query.ends_with(" | length") ||
           query == "type" || query == ".type" || query.ends_with(" | type") ||
           query == "empty" || query == ".empty" || query.ends_with(" | empty") ||
           query == "add" || query == ".add" || query.ends_with(" | add") ||
           query == "min" || query == ".min" || query.ends_with(" | min") ||
           query == "max" || query == ".max" || query.ends_with(" | max") ||
           query == "unique" || query == ".unique" || query.ends_with(" | unique") ||
           query == "reverse" || query == ".reverse" || query.ends_with(" | reverse") ||
           query == "flatten" || query == ".flatten" || query.ends_with(" | flatten") ||
           query == "to_entries" || query == ".to_entries" || query.ends_with(" | to_entries") ||
           query == "from_entries" || query == ".from_entries" || query.ends_with(" | from_entries") ||
           query == "paths" || query == ".paths" || query.ends_with(" | paths") ||
           query == "leaf_paths" || query == ".leaf_paths" || query.ends_with(" | leaf_paths") ||
           query.starts_with("has(") || query.starts_with(".has(") ||
           query.starts_with("floor(") || query.starts_with(".floor(") ||
           query.starts_with("ceil(") || query.starts_with(".ceil(") ||
           query.starts_with("round(") || query.starts_with(".round(") ||
           query.starts_with("abs(") || query.starts_with(".abs(") ||
           query.starts_with("indices(") || query.starts_with(".indices(") ||
           query.contains(" | has(") || query.contains(" | floor(") ||
           query.contains(" | ceil(") || query.contains(" | round(") ||
           query.contains(" | abs(") || query.contains(" | indices(") {
            return functions::execute_builtin_function(self, data, query);
        }
        
        // Check for logical operations first (they may contain comparisons)
        if parser::is_logical_expression(query) {
            return operators::execute_logical(self, data, query);
        }
        
        // Check for comparison operations
        if parser::is_comparison_expression(query) {
            return operators::execute_comparison(self, data, query);
        }
        
        // Check for recursive descent (..)
        if query.starts_with("..") {
            return self.execute_recursive_descent(data, query);
        }
        
        // Check for JSONPath or simple path
        if query.starts_with('.') || query == "." {
            return self.execute_path_query(data, query);
        }
        
        // Check for error function
        if query.starts_with("error(") && query.ends_with(')') {
            return self.execute_error_function(query);
        }
        
        // Check if this is a built-in function without leading dot (common in pipes)
        let simple_functions = [
            "keys", "values", "length", "type", "empty", "add", "min", "max", 
            "unique", "reverse", "flatten", "to_entries", "from_entries", 
            "paths", "leaf_paths", "floor", "ceil", "round", "abs", "sort"
        ];
        
        if simple_functions.contains(&query) {
            return functions::execute_builtin_function(self, data, query);
        }
        
        // Check for functions with arguments
        if (query.starts_with("has(") || query.starts_with("indices(") || 
            query.starts_with("split(") || query.starts_with("join(") ||
            query.starts_with("contains(") || query.starts_with("startswith(") ||
            query.starts_with("endswith(") || query.starts_with("test(") ||
            query.starts_with("match(") || query.starts_with("ltrimstr(") ||
            query.starts_with("rtrimstr(")) && query.ends_with(')') {
            return functions::execute_builtin_function(self, data, query);
        }
        
        // Check for string functions without parentheses
        if query == "trim" || query == "tostring" || query == "tonumber" ||
           query == "ascii_upcase" || query == "ascii_downcase" {
            return functions::execute_string_function(self, data, query);
        }
        
        // If none of the above, try to parse as a simple path
        self.execute_path_query(data, query)
    }
    
    pub fn execute_write(&self, data: &mut serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
        let query = query.trim();
        
        // Parse assignment queries like ".field = value"
        if let Some((path, value)) = self.parse_assignment(query)? {
            self.apply_assignment(data, &path, value)?;
            Ok(data.clone())
        } else {
            return Err(YamlQueryError::InvalidQuery(
                "Write operations currently only support simple assignments like '.field = value'".to_string()
            ));
        }
    }
    
    fn parse_assignment(&self, query: &str) -> Result<Option<(String, serde_json::Value)>, YamlQueryError> {
        // Parse simple assignment patterns like ".field = value"
        if let Some(eq_pos) = query.find('=') {
            let path = query[..eq_pos].trim();
            let value_str = query[eq_pos + 1..].trim();
            
            // Parse the value as JSON, handling different types properly
            let value = if value_str == "true" {
                serde_json::Value::Bool(true)
            } else if value_str == "false" {
                serde_json::Value::Bool(false)
            } else if value_str == "null" {
                serde_json::Value::Null
            } else if let Ok(num) = value_str.parse::<i64>() {
                serde_json::Value::Number(serde_json::Number::from(num))
            } else if let Ok(num) = value_str.parse::<f64>() {
                serde_json::Value::Number(serde_json::Number::from_f64(num).unwrap_or(serde_json::Number::from(0)))
            } else if value_str.starts_with('"') && value_str.ends_with('"') {
                // Already quoted string - parse as JSON
                serde_json::from_str(value_str)
                    .map_err(|e| YamlQueryError::InvalidQuery(format!("Invalid JSON string '{}': {}", value_str, e)))?
            } else if value_str.starts_with('[') || value_str.starts_with('{') {
                // JSON array or object
                serde_json::from_str(value_str)
                    .map_err(|e| YamlQueryError::InvalidQuery(format!("Invalid JSON '{}': {}", value_str, e)))?
            } else {
                // Treat as unquoted string
                serde_json::Value::String(value_str.to_string())
            };
            
            Ok(Some((path.to_string(), value)))
        } else {
            Ok(None)
        }
    }
    
    fn apply_assignment(&self, data: &mut serde_json::Value, path: &str, value: serde_json::Value) -> Result<(), YamlQueryError> {
        // Apply assignment to JSON data using complex path parsing
        if path == "." {
            *data = value;
            return Ok(());
        }
        
        if !path.starts_with('.') {
            return Err(YamlQueryError::InvalidQuery(
                "Assignment path must start with '.'".to_string()
            ));
        }
        
        let path = &path[1..]; // Remove leading '.'
        
        // Use the same complex path parsing logic as read operations
        self.set_complex_path(data, path, value)
    }
    
    fn set_complex_path(&self, data: &mut serde_json::Value, path: &str, value: serde_json::Value) -> Result<(), YamlQueryError> {
        let mut current = data;
        let mut i = 0;
        let chars: Vec<char> = path.chars().collect();
        
        while i < chars.len() {
            let mut segment = String::new();
            
            // Read until we hit '[', '.', or end of string
            while i < chars.len() && chars[i] != '[' && chars[i] != '.' {
                segment.push(chars[i]);
                i += 1;
            }
            
            // Check if this segment is followed by array access
            let is_array_access = i < chars.len() && chars[i] == '[';
            
            // If we have a segment, navigate to it
            if !segment.is_empty() {
                // Check if this is the final segment (no array access and at end of path)
                if !is_array_access && i >= chars.len() {
                    // This is the final segment - set the value
                    if let serde_json::Value::Object(obj) = current {
                        obj.insert(segment, value);
                        return Ok(());
                    } else {
                        return Err(YamlQueryError::ExecutionError(
                            format!("Cannot set field '{}' on non-object value", segment)
                        ));
                    }
                } else if is_array_access {
                    // This segment refers to an array - just navigate to it
                    if let serde_json::Value::Object(obj) = current {
                        current = obj.get_mut(&segment)
                            .ok_or_else(|| YamlQueryError::ExecutionError(
                                format!("Field '{}' not found", segment)
                            ))?;
                    } else {
                        return Err(YamlQueryError::ExecutionError(
                            format!("Cannot access field '{}' on non-object value", segment)
                        ));
                    }
                } else {
                    // This is an intermediate object field
                    if let serde_json::Value::Object(obj) = current {
                        // Get or create the field
                        current = obj.entry(segment.clone())
                            .or_insert(serde_json::Value::Object(serde_json::Map::new()));
                    } else {
                        return Err(YamlQueryError::ExecutionError(
                            format!("Cannot access field '{}' on non-object value", segment)
                        ));
                    }
                }
            }
            
            // Handle array access
            if is_array_access {
                i += 1; // skip '['
                let mut index_str = String::new();
                while i < chars.len() && chars[i] != ']' {
                    index_str.push(chars[i]);
                    i += 1;
                }
                if i >= chars.len() {
                    return Err(YamlQueryError::InvalidQuery("Unclosed bracket".to_string()));
                }
                i += 1; // skip ']'
                
                let index = index_str.parse::<usize>()
                    .map_err(|_| YamlQueryError::InvalidQuery(format!("Invalid array index: {}", index_str)))?;
                
                // Check if this is the final access
                if i >= chars.len() {
                    // This is the final array access - set the value
                    if let serde_json::Value::Array(arr) = current {
                        if index >= arr.len() {
                            return Err(YamlQueryError::ExecutionError(
                                format!("Array index {} out of bounds", index)
                            ));
                        }
                        arr[index] = value;
                        return Ok(());
                    } else {
                        return Err(YamlQueryError::ExecutionError(
                            "Cannot index non-array value".to_string()
                        ));
                    }
                } else {
                    // Navigate to the array element
                    if let serde_json::Value::Array(arr) = current {
                        if index >= arr.len() {
                            return Err(YamlQueryError::ExecutionError(
                                format!("Array index {} out of bounds", index)
                            ));
                        }
                        current = &mut arr[index];
                    } else {
                        return Err(YamlQueryError::ExecutionError(
                            "Cannot index non-array value".to_string()
                        ));
                    }
                }
            }
            
            // Skip dot separator
            if i < chars.len() && chars[i] == '.' {
                i += 1;
            }
        }
        
        // If we've consumed the entire path but haven't set the value yet,
        // it means we ended on a dot or the path was empty
        *current = value;
        Ok(())
    }
    
    pub fn execute_pipe_query(&self, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
        let parts = parser::parse_pipe_expression(query);
        let mut result = data.clone();
        
        for part in parts {
            let part = part.trim();
            if !part.is_empty() {
                result = self.execute(&result, part)?;
            }
        }
        
        Ok(result)
    }
    
    pub fn execute_path_query(&self, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
        let query = query.trim();
        
        // Handle root reference
        if query == "." {
            return Ok(data.clone());
        }
        
        // Handle simple wildcards for all fields (.)
        if query == ".*" {
            if let serde_json::Value::Object(obj) = data {
                let values: Vec<serde_json::Value> = obj.values().cloned().collect();
                return Ok(serde_json::Value::Array(values));
            } else {
                return Ok(serde_json::Value::Null);
            }
        }
        
        // Handle array wildcard [*]
        if query.contains("[*]") {
            return self.execute_wildcard_query(data, query);
        }
        
        if !query.starts_with('.') {
            return Err(YamlQueryError::InvalidQuery(
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
    
    fn parse_complex_path(&self, data: &serde_json::Value, path: &str) -> Result<serde_json::Value, YamlQueryError> {
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
                } else if current == serde_json::Value::Null {
                    // Trying to access a field on null should fail
                    return Err(YamlQueryError::ExecutionError(
                        format!("Cannot index null with string \"{}\"", field_name)
                    ));
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
                    return Err(YamlQueryError::InvalidQuery("Missing closing bracket ]".to_string()));
                }
                
                let index_str: String = chars[i + 1..bracket_end].iter().collect();
                let index: usize = index_str.parse()
                    .map_err(|_| YamlQueryError::InvalidQuery(format!("Invalid array index: {}", index_str)))?;
                
                if let serde_json::Value::Array(arr) = &current {
                    if index < arr.len() {
                        current = arr[index].clone();
                    } else {
                        return Ok(serde_json::Value::Null);
                    }
                } else {
                    return Ok(serde_json::Value::Null);
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
    
    fn execute_wildcard_query(&self, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
        let parts: Vec<&str> = query.split(".").collect();
        let mut current = data.clone();
        
        for part in parts {
            if part.is_empty() {
                continue;
            }
            
            if part == "*" {
                if let serde_json::Value::Object(obj) = &current {
                    let values: Vec<serde_json::Value> = obj.values().cloned().collect();
                    current = serde_json::Value::Array(values);
                } else {
                    return Ok(serde_json::Value::Null);
                }
            } else if part.contains("[*]") {
                let field = part.replace("[*]", "");
                if let serde_json::Value::Object(obj) = &current {
                    if let Some(serde_json::Value::Array(arr)) = obj.get(&field) {
                        current = serde_json::Value::Array(arr.clone());
                    } else {
                        return Ok(serde_json::Value::Null);
                    }
                } else {
                    return Ok(serde_json::Value::Null);
                }
                
                // If there are more parts after this, we need to map over the array
                // For now, just return the array itself
            } else if part.contains("[") && part.contains("]") {
                // Handle specific array index
                let bracket_start = part.find('[').unwrap();
                let bracket_end = part.find(']').unwrap();
                let field = &part[..bracket_start];
                let index_str = &part[bracket_start + 1..bracket_end];
                let index: usize = index_str.parse()
                    .map_err(|_| YamlQueryError::InvalidQuery(format!("Invalid array index: {}", index_str)))?;
                
                if let serde_json::Value::Object(obj) = &current {
                    if let Some(serde_json::Value::Array(arr)) = obj.get(field) {
                        if index < arr.len() {
                            current = arr[index].clone();
                        } else {
                            return Ok(serde_json::Value::Null);
                        }
                    } else {
                        return Ok(serde_json::Value::Null);
                    }
                } else {
                    return Ok(serde_json::Value::Null);
                }
            } else {
                // Regular field access
                if let serde_json::Value::Object(obj) = &current {
                    current = obj.get(part).unwrap_or(&serde_json::Value::Null).clone();
                } else {
                    return Ok(serde_json::Value::Null);
                }
            }
        }
        
        Ok(current)
    }
    
    fn execute_recursive_descent(&self, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
        let path = &query[2..]; // Remove ".."
        
        if path.is_empty() {
            // Return all values recursively
            let mut results = Vec::new();
            self.collect_all_values(data, &mut results);
            Ok(serde_json::Value::Array(results))
        } else {
            // Find all values with the given key recursively
            let mut results = Vec::new();
            self.collect_values_by_key(data, path, &mut results);
            if results.len() == 1 {
                Ok(results.into_iter().next().unwrap())
            } else {
                Ok(serde_json::Value::Array(results))
            }
        }
    }
    
    fn collect_all_values(&self, value: &serde_json::Value, results: &mut Vec<serde_json::Value>) {
        results.push(value.clone());
        
        match value {
            serde_json::Value::Object(obj) => {
                for val in obj.values() {
                    self.collect_all_values(val, results);
                }
            }
            serde_json::Value::Array(arr) => {
                for val in arr {
                    self.collect_all_values(val, results);
                }
            }
            _ => {}
        }
    }
    
    fn collect_values_by_key(&self, value: &serde_json::Value, key: &str, results: &mut Vec<serde_json::Value>) {
        match value {
            serde_json::Value::Object(obj) => {
                if let Some(val) = obj.get(key) {
                    results.push(val.clone());
                }
                for val in obj.values() {
                    self.collect_values_by_key(val, key, results);
                }
            }
            serde_json::Value::Array(arr) => {
                for val in arr {
                    self.collect_values_by_key(val, key, results);
                }
            }
            _ => {}
        }
    }
    
    fn execute_alternative_operator(&self, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
        let parts: Vec<&str> = query.split(" // ").collect();
        if parts.len() != 2 {
            return Err(YamlQueryError::InvalidQuery("Alternative operator (//) requires exactly two operands".to_string()));
        }
        
        let left_result = self.execute(data, parts[0].trim());
        match left_result {
            Ok(value) => {
                if value == serde_json::Value::Null {
                    self.execute_or_literal(data, parts[1].trim())
                } else {
                    Ok(value)
                }
            }
            Err(_) => self.execute_or_literal(data, parts[1].trim())
        }
    }
    
    fn execute_optional_access(&self, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
        let query_without_optional = query.replace('?', "");
        match self.execute(data, &query_without_optional) {
            Ok(value) => Ok(value),
            Err(_) => Ok(serde_json::Value::Null)
        }
    }
    
    fn execute_try_catch(&self, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
        if !query.starts_with("try ") {
            return Err(YamlQueryError::InvalidQuery("Invalid try-catch syntax".to_string()));
        }
        
        let rest = &query[4..]; // Remove "try "
        
        if let Some(catch_pos) = rest.find(" catch ") {
            let try_expr = &rest[..catch_pos].trim();
            let catch_expr = &rest[catch_pos + 7..].trim(); // 7 = len(" catch ")
            
            match self.execute(data, try_expr) {
                Ok(value) => Ok(value),
                Err(_) => {
                    // Parse catch expression - can be a literal or an expression
                    if catch_expr.starts_with('"') && catch_expr.ends_with('"') {
                        let literal = &catch_expr[1..catch_expr.len()-1];
                        Ok(serde_json::Value::String(literal.to_string()))
                    } else {
                        self.execute(data, catch_expr)
                    }
                }
            }
        } else {
            // Just try without catch
            match self.execute(data, &rest) {
                Ok(value) => Ok(value),
                Err(_) => Ok(serde_json::Value::Null)
            }
        }
    }
    
    fn execute_object_construction(&self, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
        let content = &query[1..query.len()-1]; // Remove { and }
        let mut result = serde_json::Map::new();
        
        // Simple parser for object construction
        let parts: Vec<&str> = content.split(',').map(|s| s.trim()).collect();
        
        for part in parts {
            if part.is_empty() {
                continue;
            }
            
            if let Some(colon_pos) = part.find(':') {
                let key_part = part[..colon_pos].trim();
                let value_part = part[colon_pos + 1..].trim();
                
                // Parse key (can be literal or expression)
                let key = if key_part.starts_with('"') && key_part.ends_with('"') {
                    key_part[1..key_part.len()-1].to_string()
                } else if key_part.starts_with('(') && key_part.ends_with(')') {
                    // Expression that evaluates to a key
                    let expr = &key_part[1..key_part.len()-1];
                    match self.execute(data, expr)? {
                        serde_json::Value::String(s) => s,
                        other => other.to_string().trim_matches('"').to_string(),
                    }
                } else {
                    key_part.to_string()
                };
                
                // Parse value (can be literal or expression)
                let value = if value_part.starts_with('"') && value_part.ends_with('"') {
                    serde_json::Value::String(value_part[1..value_part.len()-1].to_string())
                } else if value_part == "null" {
                    serde_json::Value::Null
                } else if value_part == "true" {
                    serde_json::Value::Bool(true)
                } else if value_part == "false" {
                    serde_json::Value::Bool(false)
                } else if let Ok(num) = value_part.parse::<i64>() {
                    serde_json::Value::Number(serde_json::Number::from(num))
                } else if let Ok(num) = value_part.parse::<f64>() {
                    serde_json::Value::Number(serde_json::Number::from_f64(num).unwrap_or(serde_json::Number::from(0)))
                } else {
                    // Treat as expression
                    self.execute(data, value_part)?
                };
                
                result.insert(key, value);
            } else {
                return Err(YamlQueryError::InvalidQuery("Invalid object construction syntax".to_string()));
            }
        }
        
        Ok(serde_json::Value::Object(result))
    }
    
    fn execute_with_entries(&self, data: &serde_json::Value, query: &str) -> Result<serde_json::Value, YamlQueryError> {
        if !query.starts_with("with_entries(") || !query.ends_with(')') {
            return Err(YamlQueryError::InvalidQuery("Invalid with_entries syntax".to_string()));
        }
        
        let expr = &query[13..query.len()-1]; // Remove "with_entries(" and ")"
        
        // First convert to entries
        let entries_result = functions::execute_builtin_function(self, data, "to_entries")?;
        
        // Then apply the expression to each entry
        if let serde_json::Value::Array(entries) = entries_result {
            let mut new_entries = Vec::new();
            for entry in entries {
                let result = self.execute(&entry, expr)?;
                new_entries.push(result);
            }
            
            // Finally convert back from entries
            let new_entries_value = serde_json::Value::Array(new_entries);
            functions::execute_builtin_function(self, &new_entries_value, "from_entries")
        } else {
            Err(YamlQueryError::ExecutionError("with_entries requires an object".to_string()))
        }
    }
    
    fn execute_error_function(&self, query: &str) -> Result<serde_json::Value, YamlQueryError> {
        let content = &query[6..query.len()-1]; // Remove "error(" and ")"
        let message = if content.starts_with('"') && content.ends_with('"') {
            content[1..content.len()-1].to_string()
        } else {
            content.to_string()
        };
        Err(YamlQueryError::ExecutionError(message))
    }
    
    pub fn execute_or_literal(&self, data: &serde_json::Value, expr: &str) -> Result<serde_json::Value, YamlQueryError> {
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
        self.execute(data, expr)
    }
}