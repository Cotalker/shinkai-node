use super::json_parsing::*;
use crate::file_parser::file_parser_types::TextGroup;
use crate::resource_errors::VRError;
use crate::source::VRSourceReference;
use crate::vector_resource::DocumentFileType;
use crate::vector_resource::SourceFileType;
use crate::vector_resource::SourceReference;

pub struct LocalFileParser {}

impl LocalFileParser {
    /// Attempts to process a file into a list of TextGroups using local processing logic
    /// implemented in Rust directly without relying on external services.
    /// If local processing is not available for the provided source, then returns Err.
    pub fn process_file_into_grouped_text(
        file_buffer: Vec<u8>,
        file_name: String,
        max_node_text_size: u64,
        source: VRSourceReference,
    ) -> Result<Vec<TextGroup>, VRError> {
        let source_base = source;

        match &source_base {
            VRSourceReference::None => Err(VRError::UnsupportedFileType(file_name.to_string())),
            VRSourceReference::Standard(source) => match source {
                SourceReference::Other(_) => Err(VRError::UnsupportedFileType(file_name.to_string())),
                SourceReference::FileRef(file_source) => match file_source.clone().file_type {
                    SourceFileType::Image(_)
                    | SourceFileType::Code(_)
                    | SourceFileType::ConfigFileType(_)
                    | SourceFileType::Video(_)
                    | SourceFileType::Audio(_)
                    | SourceFileType::Shinkai(_) => Err(VRError::UnsupportedFileType(file_name.to_string())),
                    SourceFileType::Document(doc) => match doc {
                        DocumentFileType::Json => LocalFileParser::process_json_file(
                            file_buffer,
                            file_name,
                            max_node_text_size,
                            source_base.clone(),
                        ),
                        // DocumentFileType::Csv => Self::process_csv_file(file_buffer, file_name, max_node_text_size),
                        _ => Err(VRError::UnsupportedFileType(file_name.to_string())),
                    },
                },
                SourceReference::ExternalURI(_) => Err(VRError::UnsupportedFileType(file_name.to_string())),
            },
            VRSourceReference::Notarized(_) => Err(VRError::UnsupportedFileType(file_name.to_string())),
        }
    }

    // /// Parse CSV data from a buffer and attempt to automatically detect
    // /// headers.
    // pub fn parse_csv_auto(buffer: &[u8]) -> Result<Vec<String>, VRError> {
    //     let mut reader = Reader::from_reader(Cursor::new(buffer));
    //     let headers = reader
    //         .headers()
    //         .map_err(|_| VRError::FailedCSVParsing)?
    //         .iter()
    //         .map(String::from)
    //         .collect::<Vec<String>>();

    //     let likely_header = headers.iter().all(|s| {
    //         let is_alphabetic = s.chars().all(|c| c.is_alphabetic() || c.is_whitespace());
    //         let no_duplicates = headers.iter().filter(|&item| item == s).count() == 1;
    //         let no_prohibited_chars = !s.contains(&['@', '#', '$', '%', '^', '&', '*'][..]);

    //         is_alphabetic && no_duplicates && no_prohibited_chars
    //     });

    //     Self::parse_csv(&buffer, likely_header)
    // }

    // /// Parse CSV data from a buffer.
    // /// * `header` - A boolean indicating whether to prepend column headers to
    // ///   values.
    // pub fn parse_csv(buffer: &[u8], header: bool) -> Result<Vec<String>, VRError> {
    //     let mut reader = Reader::from_reader(Cursor::new(buffer));
    //     let headers = if header {
    //         reader
    //             .headers()
    //             .map_err(|_| VRError::FailedCSVParsing)?
    //             .iter()
    //             .map(String::from)
    //             .collect::<Vec<String>>()
    //     } else {
    //         Vec::new()
    //     };

    //     let mut result = Vec::new();
    //     for record in reader.records() {
    //         let record = record.map_err(|_| VRError::FailedCSVParsing)?;
    //         let row: Vec<String> = if header {
    //             record
    //                 .iter()
    //                 .enumerate()
    //                 .map(|(i, e)| format!("{}: {}", headers[i], e))
    //                 .collect()
    //         } else {
    //             record.iter().map(String::from).collect()
    //         };
    //         let row_string = row.join(", ");
    //         result.push(row_string);
    //     }

    //     Ok(result)
    // }
}
