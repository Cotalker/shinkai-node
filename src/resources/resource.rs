use crate::db::ShinkaiDB;
use crate::resources::data_tags::{DataTag, DataTagIndex};
use crate::resources::embedding_generator::*;
use crate::resources::embeddings::*;
use crate::resources::model_type::*;
use crate::resources::resource_errors::*;
use ordered_float::NotNan;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::str::FromStr;

use super::document::DocumentResource;
use super::router::ResourcePointer;

/// Enum used for all Resources to specify their type
/// when dealing with Trait objects.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ResourceType {
    Document,
    KeyValue,
}

impl ResourceType {
    pub fn to_str(&self) -> &str {
        match self {
            ResourceType::Document => "Document",
            ResourceType::KeyValue => "KeyValue",
        }
    }
}

impl FromStr for ResourceType {
    type Err = ResourceError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Document" => Ok(ResourceType::Document),
            "KeyValue" => Ok(ResourceType::KeyValue),
            _ => Err(ResourceError::InvalidResourceType),
        }
    }
}

/// A data chunk that was retrieved from a vector search.
/// Includes extra data like the resource_id of the resource it was from
/// and the vector search score.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RetrievedDataChunk {
    pub chunk: DataChunk,
    pub score: f32,
    pub resource_pointer: ResourcePointer,
}

/// Represents a data chunk with an id, data, and optional metadata.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DataChunk {
    pub id: String,
    pub data: String,
    pub metadata: Option<String>,
    pub data_tags: Vec<DataTag>,
}

impl DataChunk {
    pub fn new(id: String, data: &str, metadata: Option<&str>, data_tags: &Vec<DataTag>) -> Self {
        Self {
            id,
            data: data.to_string(),
            metadata: metadata.map(|s| s.to_string()),
            data_tags: data_tags.clone(),
        }
    }

    pub fn new_with_integer_id(id: u64, data: &str, metadata: Option<&str>, data_tags: &Vec<DataTag>) -> Self {
        Self::new(id.to_string(), data, metadata, data_tags)
    }
}

/// Represents a Resource which includes properties and operations related to
/// data chunks and embeddings.
pub trait Resource {
    fn name(&self) -> &str;
    fn description(&self) -> Option<&str>;
    fn source(&self) -> Option<&str>;
    fn resource_id(&self) -> &str;
    fn resource_embedding(&self) -> &Embedding;
    fn set_resource_embedding(&mut self, embedding: Embedding);
    fn resource_type(&self) -> ResourceType;
    fn embedding_model_used(&self) -> EmbeddingModelType;
    fn set_embedding_model_used(&mut self, model_type: EmbeddingModelType);
    fn chunk_embeddings(&self) -> &Vec<Embedding>; // Maybe convert into hashmap in the future for efficiency
    fn data_tag_index(&self) -> &DataTagIndex;

    // Note we cannot add from_json in the trait due to trait object limitations
    // with &self.
    fn to_json(&self) -> Result<String, ResourceError>;

    /// Retrieves a data chunk given its id.
    fn get_data_chunk(&self, id: String) -> Result<&DataChunk, ResourceError>;

    /// Naively searches through all chunk embeddings in the resource
    /// to find one with a matching id
    fn get_chunk_embedding(&self, id: &str) -> Result<Embedding, ResourceError> {
        for embedding in self.chunk_embeddings() {
            if embedding.id == id {
                return Ok(embedding.clone());
            }
        }
        Err(ResourceError::InvalidChunkId)
    }

    /// Returns a String representing the Key that this Resource
    /// will be/is saved to in the Topic::Resources in the DB.
    /// The db key is: `{name}.{resource_id}`
    fn db_key(&self) -> String {
        let name = self.name().replace(" ", "_");
        let resource_id = self.resource_id().replace(" ", "_");
        format!("{}.{}", name, resource_id)
    }

    /// Regenerates and updates the resource's embedding.
    fn update_resource_embedding(
        &mut self,
        generator: &dyn EmbeddingGenerator,
        keywords: Vec<String>,
    ) -> Result<(), ResourceError> {
        let formatted = self.resource_embedding_data_formatted(keywords);
        let new_embedding = generator
            .generate_embedding_with_id(&formatted, "RE")
            .map_err(|_| ResourceError::FailedEmbeddingGeneration)?;
        self.set_resource_embedding(new_embedding);
        Ok(())
    }

    /// Generates a formatted string that represents the data to be used for the
    /// resource embedding. This string includes the resource's name,
    /// description, source, and the maximum number of keywords which can be
    /// fit.
    fn resource_embedding_data_formatted(&self, keywords: Vec<String>) -> String {
        let name = format!("Name: {}", self.name());
        let desc = self
            .description()
            .map(|description| format!(", Description: {}", description))
            .unwrap_or_default();
        let source = self
            .source()
            .map(|source| format!(", Source: {}", source))
            .unwrap_or_default();

        // Take keywords until we hit an upper 495 character cap to ensure
        // we do not go past the embedding LLM context window.
        let pre_keyword_length = name.len() + desc.len() + source.len();
        let mut keyword_string = String::new();
        for phrase in keywords {
            if pre_keyword_length + keyword_string.len() + phrase.len() <= 495 {
                keyword_string = format!("{}, {}", keyword_string, phrase);
            }
        }

        format!("{}{}{}, Keywords: [{}]", name, desc, source, keyword_string)
    }

    /// Performs a vector search using a query embedding and returns
    /// the most similar data chunks within a specific range.
    ///
    /// * `tolerance_range` - A float between 0 and 1, inclusive, that
    ///   determines the range of acceptable similarity scores as a percentage
    ///   of the highest score.
    fn vector_search_tolerance_ranged(&self, query: Embedding, tolerance_range: f32) -> Vec<RetrievedDataChunk> {
        // Get top 100 results
        let results = self.vector_search(query.clone(), 100);

        // Calculate the top similarity score
        let top_similarity_score = results.first().map_or(0.0, |ret_chunk| ret_chunk.score);

        // Now use the new function to find the range of acceptable similarity scores
        self.vector_search_tolerance_ranged_score(query, tolerance_range, top_similarity_score)
    }

    /// Performs a vector search using a query embedding and returns
    /// the most similar data chunks within a specific range of the top similarity score.
    ///
    /// * `top_similarity_score` - A float that represents the top similarity score.
    fn vector_search_tolerance_ranged_score(
        &self,
        query: Embedding,
        tolerance_range: f32,
        top_similarity_score: f32,
    ) -> Vec<RetrievedDataChunk> {
        // Clamp the tolerance_range to be between 0 and 1
        let tolerance_range = tolerance_range.max(0.0).min(1.0);

        let mut results = self.vector_search(query, 100);

        // Calculate the range of acceptable similarity scores
        let lower_bound = top_similarity_score * (1.0 - tolerance_range);

        // Filter the results to only include those within the range of the top similarity score
        results.retain(|ret_chunk| ret_chunk.score >= lower_bound && ret_chunk.score <= top_similarity_score);

        results
    }

    /// Performs a vector search using a query embedding and returns
    /// the most similar data chunks.
    fn vector_search(&self, query: Embedding, num_of_results: u64) -> Vec<RetrievedDataChunk> {
        // Fetch the ordered scores from the abstracted function
        let scores = self._vector_search_score_results(query, num_of_results);

        // Fetch the RetrievedDataChunk matching the most similar embeddings
        let mut chunks: Vec<RetrievedDataChunk> = vec![];
        for (score, id) in scores {
            if let Ok(chunk) = self.get_data_chunk(id) {
                chunks.push(RetrievedDataChunk {
                    chunk: chunk.clone(),
                    score,
                    resource_pointer: self.get_resource_pointer(),
                });
            }
        }

        chunks
    }

    /// Performs a vector search on the embeddings, and returns the properly ordered
    /// score results as a list of tuples (score, id).
    fn _vector_search_score_results(&self, query: Embedding, num_of_results: u64) -> Vec<(f32, String)> {
        let num_of_results = num_of_results as usize;

        // Calculate the similarity scores for all chunk embeddings and skip any that
        // are NaN
        let scores: Vec<(NotNan<f32>, String)> = self
            .chunk_embeddings()
            .iter()
            .filter_map(|embedding| {
                let similarity = query.cosine_similarity(embedding);
                match NotNan::new(similarity) {
                    Ok(not_nan_similarity) => Some((not_nan_similarity, embedding.id.clone())),
                    Err(_) => None, // Skip this embedding if similarity is NaN
                }
            })
            .collect();

        // Use a binary heap to more efficiently order the scores to get most similar
        let mut heap = BinaryHeap::with_capacity(num_of_results);
        for score in scores {
            if heap.len() < num_of_results {
                heap.push(Reverse(score));
            } else if let Some(least_similar_score) = heap.peek() {
                if least_similar_score.0 .0 < score.0 {
                    heap.pop();
                    heap.push(Reverse(score));
                }
            }
        }

        // Create a Vec to hold the reversed results
        let mut results: Vec<(f32, String)> = Vec::new();

        while let Some(Reverse((similarity, id))) = heap.pop() {
            results.push((similarity.into_inner(), id));
        }

        // Reverse the order of the scores so that the highest score is first
        results.reverse();

        results
    }

    /// Performs a syntactic vector search using a query embedding and a list of data tag names
    /// and returns the most similar data chunks.
    fn syntactic_vector_search(
        &self,
        query: Embedding,
        num_of_results: u64,
        data_tag_names: &Vec<String>,
    ) -> Vec<RetrievedDataChunk> {
        // TODO: Replace this with a temporal KV Resource or otherwise which is more suited for
        //       arbitrary non-ordered IDs.
        // Create a temporal Document resource to perform vector search on matching tagged data chunks
        let mut temp_doc = DocumentResource::new_empty("", None, None, "");

        // Fetch all data chunks with matching data tags and add them into temp doc
        for name in data_tag_names {
            println!("\nData tag name: {}", name);
            if let Some(ids) = self.data_tag_index().get_chunk_ids(&name) {
                println!("Matching data tag chunk ids: {:?}", ids);
                if !ids.is_empty() {
                    for id in ids {
                        if let Ok(data_chunk) = self.get_data_chunk(id.to_string()) {
                            if let Ok(embedding) = self.get_chunk_embedding(&id) {
                                println!("Appending data chunk/embedding");
                                temp_doc._manual_append_data_chunk_and_embedding(data_chunk, &embedding);
                            }
                        }
                    }
                }
            }
        }
        // Acquires similarity score of the embeddings within the temp doc
        let scores = temp_doc._vector_search_score_results(query, num_of_results);
        // Manually fetches the correct data chunks in the temp doc via iterative fetching
        let mut results: Vec<RetrievedDataChunk> = vec![];
        for (score, id) in scores {
            if let Ok(chunk) = temp_doc._get_data_chunk_iterative(id) {
                results.push(RetrievedDataChunk {
                    chunk: chunk.clone(),
                    score,
                    resource_pointer: self.get_resource_pointer(),
                });
            }
        }

        println!("Syntactic vector search results: {:?}", results);

        results
    }

    /// Generates a pointer out of the resource. Of note this is required to get around
    /// the fact that this is a trait object.
    fn get_resource_pointer(&self) -> ResourcePointer {
        let db_key = self.db_key();
        let resource_type = self.resource_type();
        let id = "1"; // This will be replaced when the ResourcePointer is added into a ResourceRouter instance
        let embedding = self.resource_embedding().clone();

        // Creating list of data tags (only name is correct, which is fine for pointers/router)
        // to add to the pointer. Unwrap is fine because guaranteed to succeed with no regex string.
        let names = self.data_tag_index().data_tag_names();
        let tags = names
            .iter()
            .cloned()
            .map(|name| DataTag::new(&name, "", "").unwrap())
            .collect::<Vec<_>>();

        ResourcePointer::new(id, &db_key, resource_type, Some(embedding), tags)
    }
}
