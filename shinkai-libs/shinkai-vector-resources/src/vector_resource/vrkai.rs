use super::{BaseVectorResource, RetrievedNode, TraversalMethod, TraversalOption, VRPath};
use crate::{
    embeddings::Embedding,
    resource_errors::VRError,
    source::{DistributionInfo, SourceFileMap},
};
use base64::{decode, encode};
use lz4_flex::{compress_prepend_size, decompress_size_prepended};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fmt;

// Versions of VRKai that are supported
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum VRKaiVersion {
    #[serde(rename = "V1")]
    V1,
}

impl VRKaiVersion {
    pub fn to_string(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

/// Represents a parsed VRKai file with a BaseVectorResource, and optional SourceFileMap.
/// To save as a file or transfer the VRKai, call one of the `prepare_as_` methods. To parse from a file/transfer, use the `from_` methods.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct VRKai {
    pub resource: BaseVectorResource,
    pub sfm: Option<SourceFileMap>,
    pub version: VRKaiVersion,
}

impl VRKai {
    /// The default VRKai version which is used when creating new VRKais
    pub fn default_vrkai_version() -> VRKaiVersion {
        VRKaiVersion::V1
    }

    /// Creates a new VRKai instance from a BaseVectorResource, with optional SourceFileMap and DistributionInfo.
    pub fn from_base_vector_resource(resource: BaseVectorResource, sfm: Option<SourceFileMap>) -> Self {
        VRKai {
            resource,
            sfm,
            version: Self::default_vrkai_version(),
        }
    }

    /// Returns the name of the Vector Resource stored in the VRKai
    pub fn name(&self) -> String {
        self.resource.as_trait_object().name().to_string()
    }

    /// Prepares the VRKai to be saved or transferred as compressed bytes.
    /// Of note, this is the bytes of the UTF-8 base64 string. This allows for easy compatibility between the two.
    pub fn encode_as_bytes(&self) -> Result<Vec<u8>, VRError> {
        if let VRKaiVersion::V1 = self.version {
            let base64_encoded = self.encode_as_base64()?;
            return Ok(base64_encoded.into_bytes());
        }
        return Err(VRError::UnsupportedVRKaiVersion(self.version.to_string()));
    }

    /// Prepares the VRKai to be saved or transferred across the network as a compressed base64 encoded string.
    pub fn encode_as_base64(&self) -> Result<String, VRError> {
        if let VRKaiVersion::V1 = self.version {
            let json_str = serde_json::to_string(self)?;
            let compressed_bytes = compress_prepend_size(json_str.as_bytes());
            let base64_encoded = encode(compressed_bytes);
            return Ok(base64_encoded);
        }
        return Err(VRError::UnsupportedVRKaiVersion(self.version.to_string()));
    }

    /// Parses a VRKai from an array of bytes, assuming the bytes are a Base64 encoded string.
    pub fn from_bytes(base64_bytes: &[u8]) -> Result<Self, VRError> {
        // If it is Version V1
        if let Ok(base64_str) = String::from_utf8(base64_bytes.to_vec())
            .map_err(|e| VRError::VRKaiParsingError(format!("UTF-8 conversion error: {}", e)))
        {
            return Self::from_base64(&base64_str);
        }

        return Err(VRError::UnsupportedVRKaiVersion("".to_string()));
    }

    /// Parses a VRKai from a Base64 encoded string.
    pub fn from_base64(base64_encoded: &str) -> Result<Self, VRError> {
        // If it is Version V1
        if let Ok(vrkai) = Self::from_base64_v1(base64_encoded) {
            return Ok(vrkai);
        }

        return Err(VRError::UnsupportedVRKaiVersion("".to_string()));
    }

    /// Parses a VRKai from a Base64 encoded string using V1 logic.
    fn from_base64_v1(base64_encoded: &str) -> Result<Self, VRError> {
        let bytes =
            decode(base64_encoded).map_err(|e| VRError::VRKaiParsingError(format!("Base64 decoding error: {}", e)))?;
        let decompressed_bytes = decompress_size_prepended(&bytes)
            .map_err(|e| VRError::VRKaiParsingError(format!("Decompression error: {}", e)))?;
        let json_str = String::from_utf8(decompressed_bytes)
            .map_err(|e| VRError::VRKaiParsingError(format!("UTF-8 conversion error: {}", e)))?;
        let vrkai = serde_json::from_str(&json_str)
            .map_err(|e| VRError::VRKaiParsingError(format!("JSON parsing error: {}", e)))?;
        Ok(vrkai)
    }

    /// Parses the VRKai into human-readable JSON (intended for readability in non-production use cases)
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Parses into a VRKai from human-readable JSON (intended for readability in non-production use cases)
    pub fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_str)
    }

    /// Performs a vector search that returns the most similar nodes based on the query with
    /// default traversal method/options.
    pub fn vector_search(&self, query: Embedding, num_of_results: u64) -> Vec<RetrievedNode> {
        self.resource.as_trait_object().vector_search(query, num_of_results)
    }

    /// Performs a vector search that returns the most similar nodes based on the query.
    /// The input traversal_method/options allows the developer to choose how the search moves through the levels.
    /// The optional starting_path allows the developer to choose to start searching from a Vector Resource
    /// held internally at a specific path.
    pub fn vector_search_customized(
        &self,
        query: Embedding,
        num_of_results: u64,
        traversal_method: TraversalMethod,
        traversal_options: &Vec<TraversalOption>,
        starting_path: Option<VRPath>,
    ) -> Vec<RetrievedNode> {
        self.resource.as_trait_object().vector_search_customized(
            query,
            num_of_results,
            traversal_method,
            traversal_options,
            starting_path,
        )
    }
}
