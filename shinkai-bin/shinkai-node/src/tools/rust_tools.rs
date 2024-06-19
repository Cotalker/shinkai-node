use crate::tools::argument::ToolArgument;
use crate::tools::error::ToolError;
use shinkai_vector_resources::embeddings::Embedding;
use shinkai_vector_resources::vector_resource::VRPath;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RustTool {
    pub name: String,
    pub description: String,
    pub input_args: Vec<ToolArgument>,
    pub tool_embedding: Embedding,
}

impl RustTool {
    pub fn new(name: String, description: String, input_args: Vec<ToolArgument>, tool_embedding: Embedding) -> Self {
        Self {
            name: VRPath::clean_string(&name),
            description,
            input_args,
            tool_embedding,
        }
    }

    /// Default name of the rust toolkit
    pub fn toolkit_type_name(&self) -> String {
        self.name.clone()
    }

    /// Convert to json
    pub fn to_json(&self) -> Result<String, ToolError> {
        serde_json::to_string(self).map_err(|_| ToolError::FailedJSONParsing)
    }

    /// Convert from json
    pub fn from_json(json: &str) -> Result<Self, ToolError> {
        let deserialized: Self = serde_json::from_str(json)?;
        Ok(deserialized)
    }
}
