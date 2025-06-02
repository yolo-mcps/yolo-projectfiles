use async_trait::async_trait;
use rust_mcp_schema::{CallToolResult, schema_utils::CallToolError};
use std::any::Any;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Shared context for stateful tools containing custom state
#[derive(Clone)]
pub struct ToolContext {
    /// Custom state bag for storing arbitrary typed data
    /// Use TypeId as key for type-safe retrieval
    pub custom_state: Arc<Mutex<HashMap<std::any::TypeId, Box<dyn Any + Send + Sync>>>>,
    /// Optional override for project root (useful for testing)
    pub project_root_override: Option<PathBuf>,
}

impl ToolContext {
    /// Create a new tool context with default values
    pub fn new() -> Self {
        Self {
            custom_state: Arc::new(Mutex::new(HashMap::new())),
            project_root_override: None,
        }
    }

    /// Create a new tool context with a project root override (useful for testing)
    pub fn with_project_root(project_root: PathBuf) -> Self {
        Self {
            custom_state: Arc::new(Mutex::new(HashMap::new())),
            project_root_override: Some(project_root),
        }
    }

    /// Get the effective project root (override or current directory)
    pub fn get_project_root(&self) -> Result<PathBuf, std::io::Error> {
        match &self.project_root_override {
            Some(root) => Ok(root.clone()),
            None => std::env::current_dir(),
        }
    }

    /// Get a typed value from custom state
    pub async fn get_custom_state<T: 'static + Send + Sync>(&self) -> Option<Arc<T>> {
        let state = self.custom_state.lock().await;
        let type_id = std::any::TypeId::of::<T>();

        state.get(&type_id)?.downcast_ref::<Arc<T>>().cloned()
    }

    /// Set a typed value in custom state
    pub async fn set_custom_state<T: 'static + Send + Sync>(&self, value: T) {
        let mut state = self.custom_state.lock().await;
        let type_id = std::any::TypeId::of::<T>();
        state.insert(type_id, Box::new(Arc::new(value)));
    }
}

impl Default for ToolContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for tools that need access to shared state
/// Tools can opt into this trait to receive context during execution
#[async_trait]
pub trait StatefulTool {
    /// Execute the tool with access to shared context
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError>;
}

/// Builder for configuring tool context with various shared resources
pub struct ToolContextBuilder {
    context: ToolContext,
}

impl ToolContextBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            context: ToolContext::new(),
        }
    }


    /// Add custom typed state
    pub async fn with_custom_state<T: 'static + Send + Sync>(self, value: T) -> Self {
        self.context.set_custom_state(value).await;
        self
    }

    /// Build the final context
    pub fn build(self) -> ToolContext {
        self.context
    }
}

impl Default for ToolContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}