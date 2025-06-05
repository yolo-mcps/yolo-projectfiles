use serde_json::Value;
use std::fmt::Debug;
use super::errors::QueryError;
use super::parser::QueryParser;
use super::operations;
use super::functions;

/// Trait for executing queries on JSON-like data
#[allow(dead_code)]
pub trait QueryExecutor {
    type Error: Debug;
    
    /// Execute a read-only query
    fn execute(&self, data: &Value, query: &str) -> Result<Value, Self::Error>;
    
    /// Execute a write query (mutates data)
    fn execute_write(&self, data: &mut Value, query: &str) -> Result<Value, Self::Error>;
}

/// Generic query engine implementation
pub struct QueryEngine {
    pub parser: QueryParser,
}

impl QueryEngine {
    pub fn new() -> Self {
        Self {
            parser: QueryParser::new(),
        }
    }
    
    /// Execute a query on the given data
    pub fn execute(&self, data: &Value, query: &str) -> Result<Value, QueryError> {
        let query = query.trim();
        
        // Handle empty query
        if query.is_empty() {
            return Ok(data.clone());
        }
        
        // Check for conditional expressions first
        if query.starts_with("if ") {
            return self.execute_conditional(data, query);
        }
        
        // Check for try-catch
        if query.starts_with("try ") {
            return self.execute_try_catch(data, query);
        }
        
        // Check for pipe operations (but not in conditionals)
        if query.contains(" | ") && !query.starts_with("if ") {
            return self.execute_pipe(data, query);
        }
        
        // Check for alternative operator
        if query.contains(" // ") {
            return self.execute_alternative(data, query);
        }
        
        // Check for optional access
        if query.ends_with('?') {
            let base_query = &query[..query.len() - 1];
            return match self.execute(data, base_query) {
                Ok(v) => Ok(v),
                Err(_) => Ok(Value::Null),
            };
        }
        
        // Check for specific operations
        if query.starts_with("with_entries(") {
            return operations::with_entries(self, data, query);
        }
        
        if query.starts_with("del(") {
            return operations::del(data, query);
        }
        
        // Check for array construction or slicing
        if query.starts_with('[') && query.ends_with(']') {
            let content = &query[1..query.len() - 1];
            // Check if it's array slicing (contains ':')
            if content.contains(':') {
                return self.execute_array_slicing(data, query);
            }
            return self.execute_array_construction(data, query);
        }
        
        // Check for object construction
        if query.starts_with('{') && query.ends_with('}') {
            return self.execute_object_construction(data, query);
        }
        
        // Check for built-in functions first (before checking expressions)
        if let Some(result) = self.try_builtin_function(data, query)? {
            return Ok(result);
        }
        
        // Check for arithmetic or comparison operations after functions
        if self.parser.is_expression(query) {
            return self.execute_expression(data, query);
        }
        
        // Check if this looks like a function call that wasn't handled
        if let Some((func_name, _)) = self.parser.parse_function_call(query) {
            // Only error if it's actually a function name pattern (not a quoted string or expression)
            if !func_name.is_empty() && !func_name.starts_with('"') {
                return Err(QueryError::FunctionNotFound(format!("Unhandled function call: {}", query)));
            }
        }
        
        // Try to parse as a literal value first (but not if it looks like a path)
        if !query.starts_with('.') && !query.starts_with("..") {
            if let Ok(value) = self.parser.parse_value(query) {
                return Ok(value);
            }
        }
        
        // Default to path query
        self.execute_path_query(data, query)
    }
    
    /// Execute a write operation
    pub fn execute_write(&self, data: &mut Value, query: &str) -> Result<Value, QueryError> {
        // Parse assignment
        if let Some((path, value)) = self.parser.parse_assignment(query)? {
            operations::set_path(data, &path, value)?;
            Ok(data.clone())
        } else {
            Err(QueryError::InvalidSyntax(
                "Write operations require assignment syntax: .path = value".to_string()
            ))
        }
    }
    
    /// Execute a conditional expression
    fn execute_conditional(&self, data: &Value, query: &str) -> Result<Value, QueryError> {
        operations::execute_conditional(self, data, query)
    }
    
    /// Execute a try-catch expression
    fn execute_try_catch(&self, data: &Value, query: &str) -> Result<Value, QueryError> {
        operations::execute_try_catch(self, data, query)
    }
    
    /// Execute a pipe expression
    fn execute_pipe(&self, data: &Value, query: &str) -> Result<Value, QueryError> {
        operations::execute_pipe(self, data, query)
    }
    
    /// Execute an alternative expression
    fn execute_alternative(&self, data: &Value, query: &str) -> Result<Value, QueryError> {
        operations::execute_alternative(self, data, query)
    }
    
    /// Execute array construction
    fn execute_array_construction(&self, data: &Value, query: &str) -> Result<Value, QueryError> {
        operations::execute_array_construction(self, data, query)
    }
    
    fn execute_array_slicing(&self, data: &Value, query: &str) -> Result<Value, QueryError> {
        operations::execute_array_slicing(self, data, query)
    }
    
    /// Execute object construction
    fn execute_object_construction(&self, data: &Value, query: &str) -> Result<Value, QueryError> {
        operations::execute_object_construction(self, data, query)
    }
    
    /// Try to execute a built-in function
    fn try_builtin_function(&self, data: &Value, query: &str) -> Result<Option<Value>, QueryError> {
        functions::try_builtin_function(self, data, query)
    }
    
    /// Execute an expression (arithmetic, comparison, etc.)
    fn execute_expression(&self, data: &Value, query: &str) -> Result<Value, QueryError> {
        operations::execute_expression(self, data, query)
    }
    
    /// Execute a path query
    fn execute_path_query(&self, data: &Value, query: &str) -> Result<Value, QueryError> {
        operations::execute_path_query(self, data, query)
    }
}

impl Default for QueryEngine {
    fn default() -> Self {
        Self::new()
    }
}