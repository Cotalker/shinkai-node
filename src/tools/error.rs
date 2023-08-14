use rocksdb::Error as RocksError;
use serde_json::Error as SerdeError;
use std::error::Error;
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum ToolError {
    RocksDBError(RocksError),
    RegexError(regex::Error),
    FailedJSONParsing,
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ToolError::RegexError(ref e) => write!(f, "Regex error: {}", e),
            ToolError::RocksDBError(ref e) => write!(f, "Rocks DB Error: {}", e),
            ToolError::FailedJSONParsing => write!(f, "Failed JSON parsing."),
        }
    }
}

impl Error for ToolError {}

impl From<RocksError> for ToolError {
    fn from(err: RocksError) -> ToolError {
        ToolError::RocksDBError(err)
    }
}

impl From<regex::Error> for ToolError {
    fn from(err: regex::Error) -> ToolError {
        ToolError::RegexError(err)
    }
}

impl From<SerdeError> for ToolError {
    fn from(error: SerdeError) -> Self {
        match error.classify() {
            serde_json::error::Category::Io => ToolError::FailedJSONParsing,
            serde_json::error::Category::Syntax => ToolError::FailedJSONParsing,
            serde_json::error::Category::Data => ToolError::FailedJSONParsing,
            serde_json::error::Category::Eof => ToolError::FailedJSONParsing,
        }
    }
}
