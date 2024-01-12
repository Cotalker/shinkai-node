use super::vector_fs_permissions::PermissionsIndex;
use super::{vector_fs::VectorFS, vector_fs_error::VectorFSError, vector_fs_reader::VFSReader};
use shinkai_message_primitives::schemas::shinkai_name::ShinkaiName;
use shinkai_vector_resources::embedding_generator::{EmbeddingGenerator, RemoteEmbeddingGenerator};
use shinkai_vector_resources::embeddings::MAX_EMBEDDING_STRING_SIZE;
use shinkai_vector_resources::vector_resource::{LimitTraversalMode, Node};
use shinkai_vector_resources::{
    embeddings::Embedding,
    vector_resource::{
        RetrievedNode, TraversalMethod, TraversalOption, VRPath, VectorResource, VectorResourceCore,
        VectorResourceSearch,
    },
};
use std::collections::HashMap;

impl VectorFS {
    /// Core method all VectorFS vector searches *must* use. Performs a vector search into the VectorFS at
    /// the specified path in reader, returning the retrieved VRHeader nodes.
    /// Automatically inspects traversal_options to guarantee folder permissions, and any other must-have options
    /// are always respected.
    fn vector_search_core(
        &self,
        reader: &VFSReader,
        query: Embedding,
        num_of_results: u64,
        traversal_method: TraversalMethod,
        traversal_options: &Vec<TraversalOption>,
    ) -> Result<Vec<RetrievedNode>, VectorFSError> {
        let mut traversal_options = traversal_options.clone();
        let internals = self._get_profile_fs_internals_read_only(&reader.profile)?;
        let stringified_permissions_map = internals
            .permissions_index
            .export_permissions_hashmap_with_reader(reader);

        // Search without unique scoring (ie. hierarchical) because "folders" have no content/real embedding.
        // Also remove any set traversal limit, so we can enforce folder permission traversal limiting.
        traversal_options.retain(|option| match option {
            TraversalOption::SetTraversalLimiting(_) | TraversalOption::SetScoringMode(_) => false,
            _ => true,
        });

        // Enforce folder permissions are respected
        traversal_options.push(TraversalOption::SetTraversalLimiting(
            LimitTraversalMode::LimitTraversalByValidationWithMap((
                _permissions_validation_func,
                stringified_permissions_map,
            )),
        ));

        let results = internals.fs_core_resource.vector_search_customized(
            query,
            num_of_results,
            traversal_method,
            &traversal_options,
            Some(reader.path.clone()),
        );

        Ok(results)
    }

    /// Performs a vector search into the VectorFS at a specific path,
    /// returning the retrieved VRHeader nodes.
    pub fn vector_search_headers(
        &self,
        reader: &VFSReader,
        query: Embedding,
        num_of_results: u64,
    ) -> Result<Vec<RetrievedNode>, VectorFSError> {
        self.vector_search_core(reader, query, num_of_results, TraversalMethod::Exhaustive, &vec![])
    }

    /// Generates an Embedding for the input query to be used in a Vector Search in the VecFS.
    /// This automatically uses the correct default embedding model for the given profile.
    pub async fn generate_query_embedding(
        &self,
        input_query: String,
        profile: &ShinkaiName,
    ) -> Result<Embedding, VectorFSError> {
        let generator = self._get_embedding_generator(profile)?;
        Ok(generator
            .generate_embedding_shorten_input_default(&input_query, MAX_EMBEDDING_STRING_SIZE as u64) // TODO: remove the hard-coding of embedding string size
            .await?)
    }
}

/// Internal validation function used by all VectorFS vector searches, in order to validate permissions of
/// VR-holding nodes while the search is traversing.
fn _permissions_validation_func(_: &Node, path: &VRPath, hashmap: HashMap<VRPath, String>) -> bool {
    // If the specified path has no permissions, then the default is to now allow traversing deeper
    if !hashmap.contains_key(path) {
        return false;
    }

    // Fetch/parse the VFSReader from the hashmap
    let reader = match hashmap.get(&PermissionsIndex::vfs_reader_unique_path()) {
        Some(reader_json) => match VFSReader::from_json(reader_json) {
            Ok(reader) => reader,
            Err(_) => return false,
        },
        None => return false,
    };
    // Initialize the PermissionsIndex struct
    let perm_index = PermissionsIndex::from_hashmap(reader.profile.clone(), hashmap);

    perm_index
        .validate_read_permission(&reader.requester_name, path)
        .is_ok()
}
