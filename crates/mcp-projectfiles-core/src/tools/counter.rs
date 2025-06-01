use crate::context::{StatefulTool, ToolContext};
use async_trait::async_trait;
use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};

#[mcp_tool(
    name = "counter_increment",
    description = "Increments a shared counter and returns the new value"
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct CounterIncrementTool {
    /// Amount to increment the counter by (defaults to 1 if not specified)
    #[serde(default = "default_increment")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub increment: Option<i32>,
}

fn default_increment() -> Option<i32> {
    None
}

#[async_trait]
impl StatefulTool for CounterIncrementTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        // Access shared counter state
        let mut counter = context.counter.lock().await;
        let increment = self.increment.unwrap_or(1);
        *counter += increment;
        let new_value = *counter;

        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                format!("Counter incremented by {} to {}", increment, new_value),
                None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}

#[mcp_tool(
    name = "counter_get",
    description = "Gets the current value of the shared counter"
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct CounterGetTool {}

#[async_trait]
impl StatefulTool for CounterGetTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        // Read shared counter state
        let counter = context.counter.lock().await;
        let current_value = *counter;

        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                format!("Current counter value: {}", current_value),
                None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}

#[mcp_tool(
    name = "cache_set",
    description = "Sets a key-value pair in the shared cache"
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct CacheSetTool {
    /// The key to set
    pub key: String,
    /// The value to store
    pub value: String,
}

#[async_trait]
impl StatefulTool for CacheSetTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        // Access shared cache
        let mut cache = context.cache.lock().await;
        cache.insert(self.key.clone(), self.value.clone());

        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                format!("Set cache[{}] = {}", self.key, self.value),
                None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}

#[mcp_tool(
    name = "cache_get",
    description = "Gets a value from the shared cache by key"
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct CacheGetTool {
    /// The key to retrieve
    pub key: String,
}

#[async_trait]
impl StatefulTool for CacheGetTool {
    async fn call_with_context(
        self,
        context: &ToolContext,
    ) -> Result<CallToolResult, CallToolError> {
        // Read from shared cache
        let cache = context.cache.lock().await;
        let value = cache.get(&self.key);

        let message = match value {
            Some(val) => format!("cache[{}] = {}", self.key, val),
            None => format!("cache[{}] = <not found>", self.key),
        };

        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                message, None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }
}