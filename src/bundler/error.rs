use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BundlerError {
    #[error("Error while extracting data")]
    Extraction(#[from] ExtractionError),
    #[error("Error while deploying bundle")]
    Deployment(#[from] DeploymentError),
}

#[derive(Debug, Error)]
pub enum ExtractionError {
    #[error("IO error encountered on path {1}")]
    Io(#[source] std::io::Error, PathBuf),
}

impl ExtractionError {
    pub fn from_io(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> Self {
        let path = path.into();
        |err| Self::Io(err, path)
    }
}

#[derive(Debug, Error)]
pub enum DeploymentError {
    #[error("IO error encountered on path {1}")]
    Io(#[source] std::io::Error, PathBuf),
    #[error("User chose not to overwrite existing directory")]
    AlreadyExists,
}

impl DeploymentError {
    pub fn from_io(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> Self {
        let path = path.into();
        |err| Self::Io(err, path)
    }
}
