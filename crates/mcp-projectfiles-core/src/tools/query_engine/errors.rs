use thiserror::Error;

#[derive(Error, Debug)]
pub enum QueryError {
    #[error("Invalid query syntax: {0}")]
    InvalidSyntax(String),
    
    #[error("Query execution failed: {0}")]
    ExecutionError(String),
    
    #[error("Type error: {0}")]
    TypeError(String),
    
    #[error("Index out of bounds: {0}")]
    IndexOutOfBounds(String),
    
    #[error("Key not found: {0}")]
    KeyNotFound(String),
    
    #[error("Division by zero")]
    DivisionByZero,
    
    #[error("Function not found: {0}")]
    FunctionNotFound(String),
    
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
}