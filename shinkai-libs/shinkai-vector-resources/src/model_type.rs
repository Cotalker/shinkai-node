use crate::resource_errors::VRError;
use serde::{Deserialize, Deserializer, Serialize, Serializer}; // pub use llm::ModelArchitecture;
use std::fmt;
use std::hash::{Hash, Hasher};

// Alias for embedding model type string
pub type EmbeddingModelTypeString = String;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Hash)]
pub enum EmbeddingModelType {
    TextEmbeddingsInference(TextEmbeddingsInference),
    OpenAI(OpenAIModelType),
}

impl EmbeddingModelType {
    /// Converts the embedding model type to a string
    pub fn to_string(&self) -> String {
        match self {
            EmbeddingModelType::TextEmbeddingsInference(model) => model.to_string(),
            EmbeddingModelType::OpenAI(model) => model.to_string(),
        }
    }

    /// Parses a string into an embedding model type
    pub fn from_string(s: &str) -> Result<Self, VRError> {
        if let Ok(model) = TextEmbeddingsInference::from_string(s) {
            return Ok(EmbeddingModelType::TextEmbeddingsInference(model));
        }
        if let Ok(model) = OpenAIModelType::from_string(s) {
            return Ok(EmbeddingModelType::OpenAI(model));
        }
        Err(VRError::InvalidModelArchitecture)
    }

    /// Returns the maximum allowed token count for an input string to be embedded, based on the embedding model
    pub fn max_input_token_count(&self) -> usize {
        match self {
            EmbeddingModelType::TextEmbeddingsInference(model) => match model {
                TextEmbeddingsInference::AllMiniLML6v2 => 510,
                TextEmbeddingsInference::AllMiniLML12v2 => 510,
                TextEmbeddingsInference::MultiQAMiniLML6 => 510,
                TextEmbeddingsInference::BgeLargeEnv1_5 => 510,
                TextEmbeddingsInference::BgeBaseEn1_5 => 510,
                TextEmbeddingsInference::EmberV1 => 510,
                TextEmbeddingsInference::GteLarge => 510,
                TextEmbeddingsInference::GteBase => 510,
                TextEmbeddingsInference::E5LargeV2 => 510,
                TextEmbeddingsInference::BgeSmallEn1_5 => 510,
                TextEmbeddingsInference::E5BaseV2 => 510,
                TextEmbeddingsInference::MultilingualE5Large => 510,
                TextEmbeddingsInference::Other(_) => 510,
            },
            EmbeddingModelType::OpenAI(model) => match model {
                OpenAIModelType::OpenAITextEmbeddingAda002 => 8190,
            },
        }
    }
}

impl fmt::Display for EmbeddingModelType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            EmbeddingModelType::TextEmbeddingsInference(model) => model.to_string().fmt(f),
            EmbeddingModelType::OpenAI(model) => model.to_string().fmt(f),
        }
    }
}

/// Hugging Face's Text Embeddings Inference Server
/// (https://github.com/huggingface/text-embeddings-inference)
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TextEmbeddingsInference {
    AllMiniLML6v2,
    AllMiniLML12v2,
    MultiQAMiniLML6,
    BgeLargeEnv1_5,
    BgeBaseEn1_5,
    EmberV1,
    GteLarge,
    GteBase,
    E5LargeV2,
    BgeSmallEn1_5,
    E5BaseV2,
    MultilingualE5Large,
    Other(String),
}
impl TextEmbeddingsInference {
    const ALL_MINI_LML6V2: &'static str =
        "sentence-transformers/all-MiniLM-L6-v2#0b6dc4ef7c29dba0d2e99a5db0c855c3102310d8";
    const ALL_MINI_LML12V2: &'static str =
        "sentence-transformers/all-MiniLM-L12-v2#a05860a77cef7b37e0048a7864658139bc18a854";
    const MULTI_QA_MINI_LML6: &'static str =
        "sentence-transformers/multi-qa-MiniLM-L6-cos-v1#2430568290bb832d22ad5064f44dd86cf0240142";
    const BGE_LARGE_ENV1_5: &'static str = "BAAI/bge-large-en-v1.5#d4aa6901d3a41ba39fb536a557fa166f842b0e09";
    const BGE_BASE_EN1_5: &'static str = "BAAI/bge-base-en-v1.5#a5beb1e3e68b9ab74eb54cfd186867f64f240e1a";
    const BGE_SMALL_EN1_5: &'static str = "BAAI/bge-small-en-v1.5#5c38ec7c405ec4b44b94cc5a9bb96e735b38267a";
    const EMBER_V1: &'static str = "llmrails/ember-v1#9e76885bed0dcfa38cbf01e1e27b3a0e8d36d4e4";
    const GTE_LARGE: &'static str = "thenlper/gte-large#58578616559541da766b9b993734f63bcfcfc057";
    const GTE_BASE: &'static str = "thenlper/gte-base#5e95d41db6721e7cbd5006e99c7508f0083223d6";
    const E5_LARGE_V2: &'static str = "intfloat/e5-large-v2#b322e09026e4ea05f42beadf4d661fb4e101d311";
    const E5_BASE_V2: &'static str = "intfloat/e5-base-v2#1c644c92ad3ba1efdad3f1451a637716616a20e8";
    const MULTILINGUAL_E5_LARGE: &'static str =
        "intfloat/multilingual-e5-large#ab10c1a7f42e74530fe7ae5be82e6d4f11a719eb";

    pub const SUPPORTED_MODELS: [&'static str; 12] = [
        Self::ALL_MINI_LML6V2,
        Self::ALL_MINI_LML12V2,
        Self::MULTI_QA_MINI_LML6,
        Self::BGE_LARGE_ENV1_5,
        Self::BGE_BASE_EN1_5,
        Self::BGE_SMALL_EN1_5,
        Self::EMBER_V1,
        Self::GTE_LARGE,
        Self::GTE_BASE,
        Self::E5_LARGE_V2,
        Self::E5_BASE_V2,
        Self::MULTILINGUAL_E5_LARGE,
    ];

    /// Returns the model name + commit in the format of "hftei/<model>#<commit>"
    fn to_string(&self) -> String {
        let model_str = match self {
            TextEmbeddingsInference::AllMiniLML6v2 => Self::ALL_MINI_LML6V2,
            TextEmbeddingsInference::AllMiniLML12v2 => Self::ALL_MINI_LML12V2,
            TextEmbeddingsInference::MultiQAMiniLML6 => Self::MULTI_QA_MINI_LML6,
            TextEmbeddingsInference::BgeLargeEnv1_5 => Self::BGE_LARGE_ENV1_5,
            TextEmbeddingsInference::BgeBaseEn1_5 => Self::BGE_BASE_EN1_5,
            TextEmbeddingsInference::BgeSmallEn1_5 => Self::BGE_SMALL_EN1_5,
            TextEmbeddingsInference::EmberV1 => Self::EMBER_V1,
            TextEmbeddingsInference::GteLarge => Self::GTE_LARGE,
            TextEmbeddingsInference::GteBase => Self::GTE_BASE,
            TextEmbeddingsInference::E5LargeV2 => Self::E5_LARGE_V2,
            TextEmbeddingsInference::E5BaseV2 => Self::E5_BASE_V2,
            TextEmbeddingsInference::MultilingualE5Large => Self::MULTILINGUAL_E5_LARGE,
            TextEmbeddingsInference::Other(name) => name,
        };
        format!("hftei/{}", model_str)
    }

    /// Parses a string in the format of "hftei/<model>#<commit>" into a TextEmbeddingsInference
    fn from_string(s: &str) -> Result<Self, VRError> {
        let stripped = s.strip_prefix("hftei/").ok_or(VRError::InvalidModelArchitecture)?;
        match stripped {
            Self::ALL_MINI_LML6V2 => Ok(TextEmbeddingsInference::AllMiniLML6v2),
            Self::ALL_MINI_LML12V2 => Ok(TextEmbeddingsInference::AllMiniLML12v2),
            Self::MULTI_QA_MINI_LML6 => Ok(TextEmbeddingsInference::MultiQAMiniLML6),
            Self::BGE_LARGE_ENV1_5 => Ok(TextEmbeddingsInference::BgeLargeEnv1_5),
            Self::BGE_BASE_EN1_5 => Ok(TextEmbeddingsInference::BgeBaseEn1_5),
            Self::BGE_SMALL_EN1_5 => Ok(TextEmbeddingsInference::BgeSmallEn1_5),
            Self::EMBER_V1 => Ok(TextEmbeddingsInference::EmberV1),
            Self::GTE_LARGE => Ok(TextEmbeddingsInference::GteLarge),
            Self::GTE_BASE => Ok(TextEmbeddingsInference::GteBase),
            Self::E5_LARGE_V2 => Ok(TextEmbeddingsInference::E5LargeV2),
            Self::E5_BASE_V2 => Ok(TextEmbeddingsInference::E5BaseV2),
            Self::MULTILINGUAL_E5_LARGE => Ok(TextEmbeddingsInference::MultilingualE5Large),
            _ => Ok(TextEmbeddingsInference::Other(stripped.to_string())),
        }
    }
}

impl fmt::Display for TextEmbeddingsInference {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

/// OpenAIModelType
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum OpenAIModelType {
    OpenAITextEmbeddingAda002,
}

impl OpenAIModelType {
    const OPENAI_TEXT_EMBEDDING_ADA_002: &'static str = "openai/text-embedding-ada-002";

    fn to_string(&self) -> String {
        match self {
            OpenAIModelType::OpenAITextEmbeddingAda002 => Self::OPENAI_TEXT_EMBEDDING_ADA_002.to_string(),
        }
    }

    fn from_string(s: &str) -> Result<OpenAIModelType, VRError> {
        match s {
            Self::OPENAI_TEXT_EMBEDDING_ADA_002 => Ok(OpenAIModelType::OpenAITextEmbeddingAda002),
            _ => Err(VRError::InvalidModelArchitecture),
        }
    }
}

impl fmt::Display for OpenAIModelType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}
