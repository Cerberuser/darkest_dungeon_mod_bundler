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
    #[error("IO error encountered on path {1} {}", .2.as_ref().map(|s| format!("({})", s)).unwrap_or_else(|| "".into()))]
    Io(#[source] std::io::Error, PathBuf, Option<String>),
}

#[macro_export]
macro_rules! io_to_extraction {
    ($path:expr) => {{
        let path = $path.into();
        move |err| {
            crate::bundler::error::ExtractionError::Io(
                err,
                path,
                Some(format!("file: {}, line: {}", file!(), line!())),
            )
        }
    }};
}
impl ExtractionError {
    pub fn from_io(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> Self {
        let path = path.into();
        |err| Self::Io(err, path, None)
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
