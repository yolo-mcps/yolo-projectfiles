use rust_mcp_schema::{
    CallToolResult, CallToolResultContentItem, TextContent, schema_utils::CallToolError,
};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use serde::{Deserialize, Serialize};

#[mcp_tool(
    name = "calculator",
    description = "Evaluates basic mathematical expressions (addition, subtraction, multiplication, division)"
)]
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
pub struct CalculatorTool {
    /// Mathematical expression to evaluate (e.g., '2 + 2', '10 * 5')
    pub expression: String,
}

impl CalculatorTool {
    pub async fn call(self) -> Result<CallToolResult, CallToolError> {
        let result = Self::evaluate_expression(&self.expression)?;

        Ok(CallToolResult {
            content: vec![CallToolResultContentItem::TextContent(TextContent::new(
                format!("{} = {}", self.expression, result),
                None,
            ))],
            is_error: Some(false),
            meta: None,
        })
    }

    fn evaluate_expression(expr: &str) -> Result<f64, CallToolError> {
        let expr = expr.trim().replace(' ', "");

        if let Ok(num) = expr.parse::<f64>() {
            return Ok(num);
        }

        if let Some(pos) = expr.rfind('+') {
            let left = Self::evaluate_expression(&expr[..pos])?;
            let right = Self::evaluate_expression(&expr[pos + 1..])?;
            return Ok(left + right);
        }

        if let Some(pos) = expr.rfind('-') {
            if pos > 0 {
                let left = Self::evaluate_expression(&expr[..pos])?;
                let right = Self::evaluate_expression(&expr[pos + 1..])?;
                return Ok(left - right);
            }
        }

        if let Some(pos) = expr.rfind('*') {
            let left = Self::evaluate_expression(&expr[..pos])?;
            let right = Self::evaluate_expression(&expr[pos + 1..])?;
            return Ok(left * right);
        }

        if let Some(pos) = expr.rfind('/') {
            let left = Self::evaluate_expression(&expr[..pos])?;
            let right = Self::evaluate_expression(&expr[pos + 1..])?;
            if right == 0.0 {
                return Err(CallToolError::unknown_tool("Division by zero".to_string()));
            }
            return Ok(left / right);
        }

        Err(CallToolError::unknown_tool(format!(
            "Invalid expression: {}",
            expr
        )))
    }
}