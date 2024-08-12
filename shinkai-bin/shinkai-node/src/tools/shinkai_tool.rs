use std::env;

use crate::tools::argument::ToolArgument;
use crate::tools::error::ToolError;
use crate::tools::js_tools::JSTool;
use crate::tools::rust_tools::RustTool;
use serde_json::{self};
use shinkai_vector_resources::embeddings::Embedding;

use super::{js_tools::JSToolWithoutCode, workflow_tool::WorkflowTool};

pub type IsEnabled = bool;


#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "content")]
pub enum ShinkaiTool {
    Rust(RustTool, IsEnabled),
    JS(JSTool, IsEnabled),
    JSLite(JSToolWithoutCode, IsEnabled), // TODO: we can get rid of this after moving to lancedb
    Workflow(WorkflowTool, IsEnabled),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ShinkaiToolHeader {
    name: String,
    description: String,
    tool_router_key: String,
    tool_type: String,
    formatted_tool_summary_for_ui: String,
}

impl ShinkaiTool {
    /// Generate a ShinkaiToolHeader from a ShinkaiTool
    pub fn to_header(&self) -> ShinkaiToolHeader {
        ShinkaiToolHeader {
            name: self.name(),
            description: self.description(),
            tool_router_key: self.tool_router_key(),
            tool_type: self.tool_type().to_string(),
            formatted_tool_summary_for_ui: self.formatted_tool_summary_for_ui(),
        }
    }

    /// The key that this tool will be stored under in the tool router
    pub fn tool_router_key(&self) -> String {
        match self {
            // so it generates name:::version
            ShinkaiTool::Workflow(w, _) => Self::gen_router_key(w.workflow.author.clone(), self.name()),
            _ => {
                let (name, toolkit_name) = (
                    self.name(),
                    match self {
                        ShinkaiTool::Rust(r, _) => r.toolkit_type_name(),
                        ShinkaiTool::JS(j, _) => j.toolkit_name.to_string(),
                        ShinkaiTool::JSLite(j, _) => j.toolkit_name.to_string(),
                        _ => unreachable!(), // This case is already handled above
                    },
                );
                Self::gen_router_key(name, toolkit_name)
            }
        }
    }

    /// Generate the key that this tool will be stored under in the tool router
    pub fn gen_router_key(name: String, toolkit_name: String) -> String {
        // We replace any `/` in order to not have the names break VRPaths
        format!("{}:::{}", toolkit_name, name).replace('/', "|")
    }

    /// Tool name
    pub fn name(&self) -> String {
        match self {
            ShinkaiTool::Rust(r, _) => r.name.clone(),
            ShinkaiTool::JS(j, _) => j.name.clone(),
            ShinkaiTool::JSLite(j, _) => j.name.clone(),
            ShinkaiTool::Workflow(w, _) => w.get_name(),
        }
    }
    /// Tool description
    pub fn description(&self) -> String {
        match self {
            ShinkaiTool::Rust(r, _) => r.description.clone(),
            ShinkaiTool::JS(j, _) => j.description.clone(),
            ShinkaiTool::JSLite(j, _) => j.description.clone(),
            ShinkaiTool::Workflow(w, _) => w.get_description(),
        }
    }

    /// Toolkit name the tool is from
    pub fn toolkit_name(&self) -> String {
        match self {
            ShinkaiTool::Rust(r, _) => r.name.clone(),
            ShinkaiTool::JS(j, _) => j.name.clone(),
            ShinkaiTool::JSLite(j, _) => j.name.clone(),
            ShinkaiTool::Workflow(w, _) => w.get_name(),
        }
    }

    /// Toolkit name the tool is from
    pub fn toolkit_type_name(&self) -> String {
        match self {
            ShinkaiTool::Rust(r, _) => r.toolkit_type_name().clone(),
            ShinkaiTool::JS(j, _) => j.toolkit_name.clone(),
            ShinkaiTool::JSLite(j, _) => j.toolkit_name.clone(),
            ShinkaiTool::Workflow(w, _) => w.get_name(),
        }
    }

    /// Returns the input arguments of the tool
    pub fn input_args(&self) -> Vec<ToolArgument> {
        match self {
            ShinkaiTool::Rust(r, _) => r.input_args.clone(),
            ShinkaiTool::JS(j, _) => j.input_args.clone(),
            ShinkaiTool::JSLite(j, _) => j.input_args.clone(),
            ShinkaiTool::Workflow(w, _) => w.get_input_args(),
        }
    }

    /// Returns the output arguments of the tool
    pub fn tool_type(&self) -> &'static str {
        match self {
            ShinkaiTool::Rust(_, _) => "Rust",
            ShinkaiTool::JS(_, _) => "JS",
            ShinkaiTool::JSLite(_, _) => "JSLite",
            ShinkaiTool::Workflow(_, _) => "Workflow",
        }
    }

    /// Returns a formatted summary of the tool
    pub fn formatted_tool_summary_for_ui(&self) -> String {
        format!(
            "Tool Name: {}\nToolkit Name: {}\nDescription: {}",
            self.name(),
            self.toolkit_type_name(),
            self.description(),
        )
    }

    /// Sets the embedding for the tool
    pub fn set_embedding(&mut self, embedding: Embedding) {
        match self {
            ShinkaiTool::Rust(r, _) => r.tool_embedding = embedding,
            ShinkaiTool::JS(j, _) => j.embedding = Some(embedding),
            ShinkaiTool::JSLite(j, _) => j.embedding = Some(embedding),
            ShinkaiTool::Workflow(w, _) => w.embedding = Some(embedding),
        }
    }

    /// Returns the tool formatted as a JSON object for the function call format
    pub fn json_function_call_format(&self) -> Result<serde_json::Value, ToolError> {
        let mut properties = serde_json::Map::new();
        let mut required_args = vec![];

        for arg in self.input_args() {
            properties.insert(
                arg.name.clone(),
                serde_json::json!({
                    "type": "string",
                    "description": arg.description.clone(),
                }),
            );
            if arg.is_required {
                required_args.push(arg.name.clone());
            }
        }

        let summary = serde_json::json!({
            "type": "function",
            "function": {
                "name": self.name(),
                "description": self.description(),
                "parameters": {
                    "type": "object",
                    "properties": properties,
                    "required": required_args,
                },
            },
        });

        Ok(summary)
    }

    pub fn json_string_function_call_format(&self) -> Result<String, ToolError> {
        let summary_value = self.json_function_call_format()?;
        serde_json::to_string(&summary_value).map_err(|_| ToolError::FailedJSONParsing)
    }

    /// Formats the tool's info into a String to be used for generating the tool's embedding.
    pub fn format_embedding_string(&self) -> String {
        format!("{} {}", self.name(), self.description())
    }

    /// Returns the embedding if it exists
    pub fn get_embedding(&self) -> Option<Embedding> {
        match self {
            ShinkaiTool::Rust(r, _) => Some(r.tool_embedding.clone()),
            ShinkaiTool::JS(j, _) => j.embedding.clone(),
            ShinkaiTool::JSLite(j, _) => j.embedding.clone(),
            ShinkaiTool::Workflow(w, _) => w.embedding.clone(),
        }
    }

    /// Returns an Option<String> for a config based on an environment variable
    pub fn get_config_from_env(&self) -> Option<String> {
        let tool_key = self.tool_router_key().replace(":::", "___");
        let env_var_key = format!("TOOLKIT_{}", tool_key);
        env::var(env_var_key).ok()
    }

    /// Check if the tool is enabled
    pub fn is_enabled(&self) -> bool {
        match self {
            ShinkaiTool::Rust(_, enabled) => *enabled,
            ShinkaiTool::JS(_, enabled) => *enabled,
            ShinkaiTool::JSLite(_, enabled) => *enabled,
            ShinkaiTool::Workflow(_, enabled) => *enabled,
        }
    }

    /// Enable the tool
    pub fn enable(&mut self) {
        match self {
            ShinkaiTool::Rust(_, enabled) => *enabled = true,
            ShinkaiTool::JS(_, enabled) => *enabled = true,
            ShinkaiTool::JSLite(_, enabled) => *enabled = true,
            ShinkaiTool::Workflow(_, enabled) => *enabled = true,
        }
    }

    /// Disable the tool
    pub fn disable(&mut self) {
        match self {
            ShinkaiTool::Rust(_, enabled) => *enabled = false,
            ShinkaiTool::JS(_, enabled) => *enabled = false,
            ShinkaiTool::JSLite(_, enabled) => *enabled = false,
            ShinkaiTool::Workflow(_, enabled) => *enabled = false,
        }
    }

    /// Convert to json
    pub fn to_json(&self) -> Result<String, ToolError> {
        serde_json::to_string(self).map_err(|_| ToolError::FailedJSONParsing)
    }

    /// Convert from json
    pub fn from_json(json: &str) -> Result<Self, ToolError> {
        let deserialized: Self = serde_json::from_str(json).map_err(|e| ToolError::ParseError(e.to_string()))?;
        Ok(deserialized)
    }
}

impl From<RustTool> for ShinkaiTool {
    fn from(tool: RustTool) -> Self {
        ShinkaiTool::Rust(tool, true)
    }
}

impl From<JSTool> for ShinkaiTool {
    fn from(tool: JSTool) -> Self {
        ShinkaiTool::JS(tool, true)
    }
}

impl From<JSToolWithoutCode> for ShinkaiTool {
    fn from(tool: JSToolWithoutCode) -> Self {
        ShinkaiTool::JSLite(tool, true)
    }
}