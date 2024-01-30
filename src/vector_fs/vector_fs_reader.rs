use crate::db::db_profile_bound::ProfileBoundWriteBatch;

use super::vector_fs::{self, VectorFS};
use super::vector_fs_error::VectorFSError;
use super::vector_fs_types::{FSEntry, FSFolder, FSItem, FSRoot, LastReadIndex};
use super::vector_fs_writer::VFSWriter;
use serde::{Deserialize, Serialize};
use shinkai_message_primitives::schemas::shinkai_name::ShinkaiName;
use shinkai_vector_resources::embeddings::MAX_EMBEDDING_STRING_SIZE;
use shinkai_vector_resources::resource_errors::VRError;
use shinkai_vector_resources::shinkai_time::ShinkaiTime;
use shinkai_vector_resources::source::{SourceFile, SourceFileMap};
use shinkai_vector_resources::vector_resource::{
    BaseVectorResource, NodeContent, RetrievedNode, VectorResource, VectorResourceCore, VectorResourceSearch,
};
use shinkai_vector_resources::{embeddings::Embedding, vector_resource::VRPath};

/// A struct that represents having access rights to read the VectorFS under a profile/at a specific path.
/// If a VFSReader struct is constructed, that means the `requester_name` has passed
/// permissions validation and is thus allowed to read `path`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct VFSReader {
    pub requester_name: ShinkaiName,
    pub path: VRPath,
    pub profile: ShinkaiName,
}

impl VFSReader {
    /// Creates a new VFSReader if the `requester_name` passes read permission validation check.
    pub fn new(
        requester_name: ShinkaiName,
        path: VRPath,
        vector_fs: &mut VectorFS,
        profile: ShinkaiName,
    ) -> Result<Self, VectorFSError> {
        let reader = VFSReader {
            requester_name: requester_name.clone(),
            path: path.clone(),
            profile: profile.clone(),
        };

        // Validate read permissions to ensure requester_name has rights
        let fs_internals = vector_fs.get_profile_fs_internals(&profile)?;
        if fs_internals
            .permissions_index
            .validate_read_permission(&requester_name, &path)
            .is_err()
        {
            return Err(VectorFSError::InvalidReaderPermission(requester_name, profile, path));
        }

        // Once permission verified, saves the datatime both into memory (last_read_index)
        // and into the FSDB as stored logs.
        let current_datetime = ShinkaiTime::generate_time_now();
        fs_internals
            .last_read_index
            .update_path_last_read(path.clone(), current_datetime, requester_name.clone());
        let mut write_batch = ProfileBoundWriteBatch::new_vfs_batch(&profile)?;
        vector_fs
            .db
            .wb_add_read_access_log(requester_name, &path, current_datetime, profile, &mut write_batch)?;
        vector_fs.db.write_pb(write_batch)?;

        Ok(reader)
    }

    /// Generates a VFSReader using the same requester_name/profile held in self.
    /// Read permissions are verified before the VFSReader is produced.
    pub fn new_reader_copied_data(&self, path: VRPath, vector_fs: &mut VectorFS) -> Result<VFSReader, VectorFSError> {
        VFSReader::new(self.requester_name.clone(), path, vector_fs, self.profile.clone())
    }

    /// Generates a VFSWriter using the same requester_name/profile held in self.
    /// Write permissions are verified before the VFSWriter is produced.
    pub fn new_writer_copied_data(&self, path: VRPath, vector_fs: &mut VectorFS) -> Result<VFSWriter, VectorFSError> {
        VFSWriter::new(self.requester_name.clone(), path, vector_fs, self.profile.clone())
    }

    /// Serialize the PathPermission struct into a JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize a JSON string into a PathPermission struct
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

impl VectorFS {
    /// Retrieves the FSEntry for the reader's path in the VectorFS. If path is root `/`, then returns a
    /// FSFolder that matches the FS root structure.
    pub fn retrieve_fs_entry(&mut self, reader: &VFSReader) -> Result<FSEntry, VectorFSError> {
        let internals = self.get_profile_fs_internals_read_only(&reader.profile)?;

        // Create FSRoot directly if path is root
        if reader.path.is_empty() {
            let fs_root =
                FSRoot::from_core_vector_resource(internals.fs_core_resource.clone(), &internals.last_read_index)?;
            return Ok(FSEntry::Root(fs_root));
        }

        // Otherwise retrieve the node and process it
        let ret_node = internals.fs_core_resource.retrieve_node_at_path(reader.path.clone())?;
        match ret_node.node.content {
            NodeContent::Resource(_) => {
                let fs_folder = FSFolder::from_vector_resource_node(
                    ret_node.node.clone(),
                    reader.path.clone(),
                    &internals.last_read_index,
                )?;
                Ok(FSEntry::Folder(fs_folder))
            }
            NodeContent::VRHeader(_) => {
                let fs_item =
                    FSItem::from_vr_header_node(ret_node.node, reader.path.clone(), &internals.last_read_index)?;
                Ok(FSEntry::Item(fs_item))
            }
            _ => Ok(Err(VRError::InvalidNodeType(ret_node.node.id))?),
        }
    }

    /// Attempts to retrieve a VectorResource from inside an FSItem at the path specified in reader. If an FSItem/VectorResource is not saved
    /// at this path, an error will be returned.
    pub fn retrieve_vector_resource(&mut self, reader: &VFSReader) -> Result<BaseVectorResource, VectorFSError> {
        let fs_item = self.retrieve_fs_entry(reader)?.as_item()?;
        self.db.get_resource_by_fs_item(&fs_item, &reader.profile)
    }

    /// Attempts to retrieve the SourceFileMap from inside an FSItem at the path specified in reader. If this path does not currently exist, or
    /// a source_file is not saved at this path, then an error is returned.
    pub fn retrieve_source_file_map(&mut self, reader: &VFSReader) -> Result<SourceFileMap, VectorFSError> {
        let fs_item = self.retrieve_fs_entry(reader)?.as_item()?;
        self.db.get_source_file_map_by_fs_item(&fs_item, &reader.profile)
    }

    /// Attempts to retrieve a VectorResource and its SourceFileMap at the path specified in reader. If either is not available
    /// an error will be returned.
    pub fn retrieve_vr_and_source_file_map(
        &mut self,
        reader: &VFSReader,
    ) -> Result<(BaseVectorResource, SourceFileMap), VectorFSError> {
        let vr = self.retrieve_vector_resource(reader)?;
        let sfm = self.retrieve_source_file_map(reader)?;
        Ok((vr, sfm))
    }

    /// Attempts to retrieve a VectorResource from inside an FSItem within the folder specified at reader path.
    /// If a VectorResource is not saved at this path, an error will be returned.
    pub fn retrieve_vector_resource_in_folder(
        &mut self,
        reader: &VFSReader,
        item_name: String,
    ) -> Result<BaseVectorResource, VectorFSError> {
        let new_reader = reader.new_reader_copied_data(reader.path.push_cloned(item_name), self)?;
        self.retrieve_vector_resource(&new_reader)
    }

    /// Attempts to retrieve a SourceFileMap from inside an FSItem within the folder specified at reader path.
    /// If this path does not currently exist, or a source_file is not saved at this path,
    /// then an error is returned.
    pub fn retrieve_source_file_map_in_folder(
        &mut self,
        reader: &VFSReader,
        item_name: String,
    ) -> Result<SourceFileMap, VectorFSError> {
        let new_reader = reader.new_reader_copied_data(reader.path.push_cloned(item_name), self)?;
        self.retrieve_source_file_map(&new_reader)
    }

    /// Attempts to retrieve a VectorResource and its SourceFileMap from inside an FSItem within the folder specified at reader path.
    /// If either is not available, an error will be returned.
    pub fn retrieve_vr_and_source_file_map_in_folder(
        &mut self,
        reader: &VFSReader,
        item_name: String,
    ) -> Result<(BaseVectorResource, SourceFileMap), VectorFSError> {
        let new_reader = reader.new_reader_copied_data(reader.path.push_cloned(item_name), self)?;
        self.retrieve_vr_and_source_file_map(&new_reader)
    }

    /// Retrieves a node at a given path from the VectorFS core resource under a profile
    pub fn _retrieve_core_resource_node_at_path(
        &self,
        path: VRPath,
        profile: &ShinkaiName,
    ) -> Result<RetrievedNode, VectorFSError> {
        let internals = self.get_profile_fs_internals_read_only(profile)?;
        internals
            .fs_core_resource
            .retrieve_node_at_path(path.clone())
            .map_err(|_| VectorFSError::NoEntryAtPath(path.clone()))
    }

    /// Validates that the path points to a FSFolder
    pub fn validate_path_points_to_folder(&self, path: VRPath, profile: &ShinkaiName) -> Result<(), VectorFSError> {
        let ret_node = self._retrieve_core_resource_node_at_path(path.clone(), profile)?;

        match ret_node.node.content {
            NodeContent::Resource(_) => Ok(()),
            _ => Err(VectorFSError::PathDoesNotPointAtFolder(path)),
        }
    }

    /// Validates that the path points to a FSItem
    pub fn validate_path_points_to_item(&self, path: VRPath, profile: &ShinkaiName) -> Result<(), VectorFSError> {
        let ret_node = self._retrieve_core_resource_node_at_path(path.clone(), profile)?;

        match ret_node.node.content {
            NodeContent::VRHeader(_) => Ok(()),
            _ => Err(VectorFSError::PathDoesNotPointAtItem(path.clone())),
        }
    }

    /// Validates that the path points to any FSEntry, meaning that something exists at that path
    pub fn validate_path_points_to_entry(&self, path: VRPath, profile: &ShinkaiName) -> Result<(), VectorFSError> {
        self._retrieve_core_resource_node_at_path(path, profile).map(|_| ())
    }
}
