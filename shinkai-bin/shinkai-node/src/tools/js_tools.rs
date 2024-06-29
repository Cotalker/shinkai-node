use std::thread;

use super::js_toolkit_headers::ToolConfig;
use crate::tools::argument::ToolArgument;
use crate::tools::error::ToolError;
use serde_json::Value as JsonValue;
use shinkai_tools_runner::tools::run_result::RunResult;
use shinkai_tools_runner::tools::tool::Tool;
use shinkai_vector_resources::embeddings::Embedding;
use tokio::runtime::Runtime;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct JSTool {
    pub toolkit_name: String,
    pub name: String,
    pub author: String,
    pub js_code: String,
    pub config: Vec<ToolConfig>,
    pub description: String,
    pub keywords: Vec<String>,
    pub input_args: Vec<ToolArgument>,
    pub config_set: bool,
    pub activated: bool,
    pub embedding: Option<Embedding>,
}

impl JSTool {
    pub fn run(&self, input_json: JsonValue) -> Result<RunResult, ToolError> {
        let code = self.js_code.clone();
        let config = serde_json::to_string(&self.config).map_err(|e| ToolError::SerializationError(e.to_string()))?;
        let input = serde_json::to_string(&input_json).map_err(|e| ToolError::SerializationError(e.to_string()))?;

        // Create a new thread with its own Tokio runtime
        thread::spawn(move || {
            let rt = Runtime::new().expect("Failed to create Tokio runtime");
            rt.block_on(async {
                let mut tool = Tool::new();
                tool.load_from_code(&code, &config)
                    .await
                    .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
                tool.run(&input)
                    .await
                    .map_err(|e| ToolError::ExecutionError(e.to_string()))
            })
        })
        .join()
        .expect("Thread panicked")
    }

    /// Convert to JSON string
    pub fn to_json_string(&self) -> Result<String, ToolError> {
        serde_json::to_string(self).map_err(|e| ToolError::SerializationError(e.to_string()))
    }

    /// Convert to JSToolWithoutCode
    pub fn to_without_code(&self) -> JSToolWithoutCode {
        JSToolWithoutCode {
            toolkit_name: self.toolkit_name.clone(),
            name: self.name.clone(),
            author: self.author.clone(),
            config: self.config.clone(),
            description: self.description.clone(),
            keywords: self.keywords.clone(),
            input_args: self.input_args.clone(),
            config_set: self.config_set,
            activated: self.activated,
            embedding: self.embedding.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct JSToolWithoutCode {
    pub toolkit_name: String,
    pub name: String,
    pub author: String,
    pub config: Vec<ToolConfig>,
    pub description: String,
    pub keywords: Vec<String>,
    pub input_args: Vec<ToolArgument>,
    pub config_set: bool,
    pub activated: bool,
    pub embedding: Option<Embedding>,
}

impl JSToolWithoutCode {
    pub fn from_jstool(tool: &JSTool) -> Self {
        JSToolWithoutCode {
            toolkit_name: tool.toolkit_name.clone(),
            name: tool.name.clone(),
            author: tool.author.clone(),
            config: tool.config.clone(),
            description: tool.description.clone(),
            keywords: tool.keywords.clone(),
            input_args: tool.input_args.clone(),
            config_set: tool.config_set,
            activated: tool.activated,
            embedding: tool.embedding.clone(),
        }
    }
}
