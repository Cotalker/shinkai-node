use std::collections::HashMap;

use super::{
    BaseVectorResource, MapVectorResource, Node, NodeContent, RetrievedNode, ScoringMode, TraversalMethod,
    TraversalOption, VRKai, VRPath, VRSource,
};
#[cfg(feature = "native-http")]
use crate::embedding_generator::{EmbeddingGenerator, RemoteEmbeddingGenerator};
use crate::model_type::EmbeddingModelType;
use crate::{embeddings::Embedding, resource_errors::VRError};
use base64::{decode, encode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::Value as JsonValue;

// Versions of VRPack that are supported
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum VRPackVersion {
    #[serde(rename = "V1")]
    V1,
}

impl VRPackVersion {
    pub fn to_string(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

// Wrapper for embedding model type string inside of the hashmap key
type EmbeddingModelTypeString = String;

/// Represents a parsed VRPack file, which contains a Map Vector Resource that holds a tree structure of folders & encoded VRKai nodes.
/// In other words, a `.vrpack` file is akin to a "compressed archive" of internally held VRKais with folder structure preserved.
/// Of note, VRPacks are not compressed at the top level because the VRKais held inside already are. This improves performance for large VRPacks.
/// To save as a file or transfer the VRPack, call one of the `encode_as_` methods. To parse from a file/transfer, use the `from_` methods.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct VRPack {
    pub name: String,
    pub resource: BaseVectorResource,
    pub version: VRPackVersion,
    pub vrkai_count: u64,
    pub folder_count: u64,
    pub embedding_models_used: HashMap<EmbeddingModelTypeString, u64>,
}

impl VRPack {
    /// The default VRPack version which is used when creating new VRPacks
    pub fn default_vrpack_version() -> VRPackVersion {
        VRPackVersion::V1
    }

    /// Creates a new VRPack with the provided BaseVectorResource and the default version.
    pub fn new(
        name: &str,
        resource: BaseVectorResource,
        embedding_models_used: HashMap<EmbeddingModelTypeString, u64>,
    ) -> Self {
        let (vrkai_count, folder_count) = Self::num_of_vrkais_and_folders(&resource);

        VRPack {
            name: name.to_string(),
            resource,
            version: Self::default_vrpack_version(),
            vrkai_count,
            folder_count,
            embedding_models_used,
        }
    }

    /// Creates a new empty VRPack with an empty BaseVectorResource and the default version.
    pub fn new_empty(name: &str) -> Self {
        VRPack {
            name: name.to_string(),
            resource: BaseVectorResource::Map(MapVectorResource::new_empty("vrpack", None, VRSource::None, true)),
            version: Self::default_vrpack_version(),
            vrkai_count: 0,
            folder_count: 0,
            embedding_models_used: HashMap::new(),
        }
    }

    /// Prepares the VRPack to be saved or transferred as bytes.
    /// Of note, this is the bytes of the UTF-8 base64 string. This allows for easy compatibility between the two.
    pub fn encode_as_bytes(&self) -> Result<Vec<u8>, VRError> {
        if let VRPackVersion::V1 = self.version {
            let base64_encoded = self.encode_as_base64()?;
            return Ok(base64_encoded.into_bytes());
        }
        return Err(VRError::UnsupportedVRPackVersion(self.version.to_string()));
    }

    /// Prepares the VRPack to be saved or transferred across the network as a base64 encoded string.
    pub fn encode_as_base64(&self) -> Result<String, VRError> {
        if let VRPackVersion::V1 = self.version {
            let json_str = serde_json::to_string(self)?;
            let base64_encoded = encode(json_str.as_bytes());
            return Ok(base64_encoded);
        }
        return Err(VRError::UnsupportedVRPackVersion(self.version.to_string()));
    }

    /// Parses a VRPack from an array of bytes, assuming the bytes are a Base64 encoded string.
    pub fn from_bytes(base64_bytes: &[u8]) -> Result<Self, VRError> {
        // If it is Version V1
        if let Ok(base64_str) = String::from_utf8(base64_bytes.to_vec())
            .map_err(|e| VRError::VRPackParsingError(format!("UTF-8 conversion error: {}", e)))
        {
            return Self::from_base64(&base64_str);
        }

        return Err(VRError::UnsupportedVRPackVersion("".to_string()));
    }

    /// Parses a VRPack from a Base64 encoded string without compression.
    pub fn from_base64(base64_encoded: &str) -> Result<Self, VRError> {
        // If it is Version V1
        let v1 = Self::from_base64_v1(base64_encoded);
        if let Ok(vrkai) = v1 {
            return Ok(vrkai);
        } else if let Err(e) = v1 {
            println!("Error: {}", e);
        }

        return Err(VRError::UnsupportedVRPackVersion("".to_string()));
    }

    /// Parses a VRPack from a Base64 encoded string using V1 logic without compression.
    fn from_base64_v1(base64_encoded: &str) -> Result<Self, VRError> {
        let bytes =
            decode(base64_encoded).map_err(|e| VRError::VRPackParsingError(format!("Base64 decoding error: {}", e)))?;
        let json_str = String::from_utf8(bytes)
            .map_err(|e| VRError::VRPackParsingError(format!("UTF-8 conversion error: {}", e)))?;
        let vrkai = serde_json::from_str(&json_str)
            .map_err(|e| VRError::VRPackParsingError(format!("JSON parsing error: {}", e)))?;
        Ok(vrkai)
    }

    /// Parses the VRPack into human-readable JSON (intended for readability in non-production use cases)
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Parses into a VRPack from human-readable JSON (intended for readability in non-production use cases)
    pub fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_str)
    }

    /// Sets the name of the VRPack.
    pub fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
    }

    /// Sets the resource of the VRPack.
    pub fn set_resource(
        &mut self,
        resource: BaseVectorResource,
        embedding_models_used: HashMap<EmbeddingModelTypeString, u64>,
    ) {
        let (vrkai_count, folder_count) = Self::num_of_vrkais_and_folders(&resource);
        self.resource = resource;
        self.vrkai_count = vrkai_count;
        self.folder_count = folder_count;
        self.embedding_models_used = embedding_models_used;
    }

    /// Returns the ID of the VRPack.
    pub fn id(&self) -> String {
        self.resource.as_trait_object().resource_id().to_string()
    }

    /// Returns the Merkle root of the VRPack.
    pub fn merkle_root(&self) -> Result<String, VRError> {
        self.resource.as_trait_object().get_merkle_root()
    }

    /// Adds a VRKai into the VRPack inside of the specified parent path (folder or root).
    pub fn insert_vrkai(&mut self, vrkai: &VRKai, parent_path: VRPath) -> Result<(), VRError> {
        let resource_name = vrkai.resource.as_trait_object().name().to_string();
        let embedding = vrkai.resource.as_trait_object().resource_embedding().clone();
        let metadata = None;
        let enc_vrkai = vrkai.encode_as_base64()?;
        let mut node = Node::new_text(resource_name.clone(), enc_vrkai, metadata, &vec![]);
        node.merkle_hash = Some(vrkai.resource.as_trait_object().get_merkle_root()?);

        self.resource
            .as_trait_object_mut()
            .insert_node_at_path(parent_path, resource_name, node, embedding)?;

        // Add the embedding model used to the hashmap
        let model = vrkai.resource.as_trait_object().embedding_model_used();
        if !self.embedding_models_used.contains_key(&model.to_string()) {
            self.embedding_models_used
                .entry(model.to_string())
                .and_modify(|count| *count += 1)
                .or_insert(1);
        }
        self.vrkai_count += 1;

        Ok(())
    }

    /// Creates a folder inside the VRPack at the specified parent path.
    pub fn create_folder(&mut self, folder_name: &str, parent_path: VRPath) -> Result<(), VRError> {
        let resource = BaseVectorResource::Map(MapVectorResource::new_empty(folder_name, None, VRSource::None, true));
        let node = Node::new_vector_resource(folder_name.to_string(), &resource, None);
        let embedding = Embedding::new_empty();

        self.resource.as_trait_object_mut().insert_node_at_path(
            parent_path,
            folder_name.to_string(),
            node,
            embedding,
        )?;

        self.folder_count += 1;

        Ok(())
    }

    /// Parses a node into a VRKai.
    fn parse_node_to_vrkai(node: &Node) -> Result<VRKai, VRError> {
        match &node.content {
            NodeContent::Text(content) => {
                return VRKai::from_base64(content);
            }
            _ => Err(VRError::VRKaiParsingError("Invalid node content type".to_string())),
        }
    }

    /// Fetches the VRKai node at the specified path and parses it into a VRKai.
    pub fn get_vrkai(&self, path: VRPath) -> Result<VRKai, VRError> {
        let node = self.resource.as_trait_object().retrieve_node_at_path(path.clone())?;
        Self::parse_node_to_vrkai(&node.node)
    }

    /// Removes a node (VRKai or folder) from the VRPack at the specified path.
    pub fn remove_at_path(&mut self, path: VRPath) -> Result<(), VRError> {
        let removed_node = self.resource.as_trait_object_mut().remove_node_at_path(path)?;
        match removed_node.0.content {
            NodeContent::Text(vrkai_base64) => {
                // Decrease the embedding model count in the hashmap
                let vrkai = VRKai::from_base64(&vrkai_base64)?;
                let model = vrkai.resource.as_trait_object().embedding_model_used();
                if let Some(count) = self.embedding_models_used.get_mut(&model.to_string()) {
                    if *count > 1 {
                        *count -= 1;
                    } else {
                        self.embedding_models_used.remove(&model.to_string());
                    }
                }
                // Decrease vrkai count
                self.vrkai_count -= 1;
            }
            NodeContent::Resource(_) => self.folder_count += 1,
            _ => (),
        }
        Ok(())
    }

    /// Unpacks all VRKais in the VRPack, each as a tuple containing a VRKai and its corresponding VRPath where it was held at.
    pub fn unpack_all_vrkais(&self) -> Result<Vec<(VRKai, VRPath)>, VRError> {
        let nodes = self.resource.as_trait_object().retrieve_nodes_exhaustive(None, false);

        let mut vrkais_with_paths = Vec::new();
        for retrieved_node in nodes {
            match retrieved_node.node.content {
                NodeContent::Text(_) => match Self::parse_node_to_vrkai(&retrieved_node.node) {
                    Ok(vrkai) => vrkais_with_paths.push((vrkai, retrieved_node.retrieval_path.clone())),
                    Err(e) => return Err(e),
                },
                _ => continue,
            }
        }

        Ok(vrkais_with_paths)
    }

    /// Prints the internal structure of the VRPack, starting from a given path.
    pub fn print_internal_structure(&self, starting_path: Option<VRPath>) {
        println!("{} VRPack Internal Structure:", self.name);
        println!("------------------------------------------------------------");
        let nodes = self
            .resource
            .as_trait_object()
            .retrieve_nodes_exhaustive(starting_path, false);
        for node in nodes {
            let ret_path = node.retrieval_path;
            let path = ret_path.format_to_string();
            let path_depth = ret_path.path_ids.len();
            let data = match &node.node.content {
                NodeContent::Text(s) => {
                    let text_content = if s.chars().count() > 25 {
                        s.chars().take(25).collect::<String>() + "..."
                    } else {
                        s.to_string()
                    };
                    format!("VRKai: {}", node.node.id)
                }
                NodeContent::Resource(resource) => {
                    if path_depth == 1 {
                        println!(" ");
                    }
                    format!(
                        "{} <Folder> - {} Nodes Held Inside",
                        resource.as_trait_object().name(),
                        resource.as_trait_object().get_root_embeddings().len()
                    )
                }
                _ => continue, // Skip ExternalContent and VRHeader
            };
            // Adding merkle hash if it exists to output string
            let mut merkle_hash = String::new();
            if let Ok(hash) = node.node.get_merkle_hash() {
                if hash.chars().count() > 15 {
                    merkle_hash = hash.chars().take(15).collect::<String>() + "..."
                } else {
                    merkle_hash = hash.to_string()
                }
            }

            // Create indent string and do the final print
            let indent_string = " ".repeat(path_depth * 2) + &">".repeat(path_depth);
            if merkle_hash.is_empty() {
                println!("{}{}", indent_string, data);
            } else {
                println!("{}{} | Merkle Hash: {}", indent_string, data, merkle_hash);
            }
        }
    }

    #[cfg(feature = "native-http")]
    /// Performs a dynamic vector search within the VRPack and returns the most similar VRKais based on the input query String.
    pub async fn dynamic_vector_search_vrkai(
        &self,
        input_query: String,
        num_of_results: u64,
        embedding_generator: RemoteEmbeddingGenerator,
    ) -> Result<Vec<VRKai>, VRError> {
        self.dynamic_vector_search_vrkai_customized(input_query, num_of_results, &vec![], None, embedding_generator)
            .await
    }

    #[cfg(feature = "native-http")]
    /// Performs a dynamic vector search within the VRPack and returns the most similar VRKais based on the input query String.
    /// Supports customizing the search starting path/traversal options.
    pub async fn dynamic_vector_search_vrkai_customized(
        &self,
        input_query: String,
        num_of_results: u64,
        traversal_options: &Vec<TraversalOption>,
        starting_path: Option<VRPath>,
        embedding_generator: RemoteEmbeddingGenerator,
    ) -> Result<Vec<VRKai>, VRError> {
        let retrieved_nodes = self
            .resource
            .as_trait_object()
            .dynamic_vector_search_customized(
                input_query,
                num_of_results,
                traversal_options,
                starting_path,
                embedding_generator,
            )
            .await?;

        let vrkais: Vec<VRKai> = retrieved_nodes
            .into_iter()
            .filter_map(|node| Self::parse_node_to_vrkai(&node.node).ok())
            .collect();

        Ok(vrkais)
    }

    #[cfg(feature = "native-http")]
    /// Performs a deep vector search within the VRPack, returning the highest scored `RetrievedNode`s across
    /// the VRKais stored in the VRPack.
    pub async fn deep_vector_search(
        &self,
        input_query: String,
        num_of_vrkais_to_search_into: u64,
        num_of_results: u64,
        embedding_generator: RemoteEmbeddingGenerator,
    ) -> Result<Vec<RetrievedNode>, VRError> {
        self.deep_vector_search_customized(
            input_query,
            num_of_vrkais_to_search_into,
            &vec![],
            None,
            num_of_results,
            TraversalMethod::Exhaustive,
            &vec![TraversalOption::SetScoringMode(ScoringMode::HierarchicalAverageScoring)],
            embedding_generator,
        )
        .await
    }

    #[cfg(feature = "native-http")]
    /// Performs a deep vector search within the VRPack, returning the highest scored `RetrievedNode`s across
    /// the VRKais stored in the VRPack. Customized allows specifying options for the first top-level search for VRKais,
    /// and then "deep" options/method for the vector searches into the VRKais to acquire the `RetrievedNode`s.
    pub async fn deep_vector_search_customized(
        &self,
        input_query: String,
        num_of_vrkais_to_search_into: u64,
        traversal_options: &Vec<TraversalOption>,
        vr_pack_starting_path: Option<VRPath>,
        num_of_results: u64,
        deep_traversal_method: TraversalMethod,
        deep_traversal_options: &Vec<TraversalOption>,
        embedding_generator: RemoteEmbeddingGenerator,
    ) -> Result<Vec<RetrievedNode>, VRError> {
        let vrkais = self
            .dynamic_vector_search_vrkai_customized(
                input_query.clone(),
                num_of_vrkais_to_search_into,
                traversal_options,
                vr_pack_starting_path.clone(),
                embedding_generator.clone(),
            )
            .await?;

        let mut retrieved_nodes = Vec::new();
        // Perform vector search on all VRKai resources
        for vrkai in vrkais {
            let query_embedding = embedding_generator.generate_embedding_default(&input_query).await?;
            let results = vrkai.resource.as_trait_object().vector_search_customized(
                query_embedding,
                num_of_results,
                deep_traversal_method.clone(),
                &deep_traversal_options,
                None,
            );
            retrieved_nodes.extend(results);
        }

        // Sort the retrieved nodes by score before returning
        let sorted_retrieved_nodes = RetrievedNode::sort_by_score(&retrieved_nodes, num_of_results);

        Ok(sorted_retrieved_nodes)
    }

    /// Counts the number of VRKais and folders in the BaseVectorResource.
    fn num_of_vrkais_and_folders(resource: &BaseVectorResource) -> (u64, u64) {
        let nodes = resource.as_trait_object().retrieve_nodes_exhaustive(None, false);

        let (vrkais_count, folders_count) = nodes.iter().fold((0u64, 0u64), |(vrkais, folders), retrieved_node| {
            match retrieved_node.node.content {
                NodeContent::Text(_) => (vrkais + 1, folders),
                NodeContent::Resource(_) => (vrkais, folders + 1),
                _ => (vrkais, folders),
            }
        });

        (vrkais_count, folders_count)
    }

    /// Generates a simplified JSON representation of the contents of the VRPack.
    pub fn to_json_contents_simplified(&self) -> Result<String, VRError> {
        let nodes = self.resource.as_trait_object().retrieve_nodes_exhaustive(None, false);

        let mut content_vec = Vec::new();

        for retrieved_node in nodes {
            let ret_path = retrieved_node.retrieval_path;
            let path = ret_path.format_to_string();
            let path_depth = ret_path.path_ids.len();

            // TODO: add merkle hashes to output vrkais
            let json_node = match &retrieved_node.node.content {
                NodeContent::Text(_) => {
                    json!({
                        "name": retrieved_node.node.id,
                        "type": "vrkai",
                        "path": path,
                        "merkle_hash": retrieved_node.node.get_merkle_hash().unwrap_or_default(),
                    })
                }
                NodeContent::Resource(_) => {
                    json!({
                        "name": retrieved_node.node.id,
                        "type": "folder",
                        "path": path,
                        "contents": [],
                    })
                }
                _ => continue,
            };

            if path_depth == 0 {
                content_vec.push(json_node);
            } else {
                let parent_path = ret_path.parent_path().format_to_string();
                Self::insert_node_into_json_vec(&mut content_vec, parent_path, json_node);
            }
        }

        let simplified_json = json!({
            "name": self.name,
            "vrkai_count": self.vrkai_count,
            "folder_count": self.folder_count,
            "version": self.version.to_string(),
            "content": content_vec,
            "embedding_models_used": self.embedding_models_used,
        });

        serde_json::to_string(&simplified_json)
            .map_err(|e| VRError::VRPackParsingError(format!("JSON serialization error: {}", e)))
    }

    fn insert_node_into_json_vec(content_vec: &mut Vec<JsonValue>, parent_path: String, json_node: JsonValue) {
        for node in content_vec.iter_mut() {
            if let Some(path) = node["path"].as_str() {
                if path == parent_path {
                    if let Some(contents) = node["contents"].as_array_mut() {
                        contents.push(json_node);
                        return;
                    }
                } else if parent_path.starts_with(path) {
                    if let Some(contents) = node["contents"].as_array_mut() {
                        Self::insert_node_into_json_vec(contents, parent_path, json_node);
                        return;
                    }
                }
            }
        }
        // If the parent node is not found, it means the json_node should be added to the root content_vec
        content_vec.push(json_node);
    }
}
