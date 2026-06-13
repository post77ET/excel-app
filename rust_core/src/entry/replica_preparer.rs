use std::fs;
use std::path::Path;

use crate::entry::entry_state::JobPaths;
use crate::infra::app_error::AppError;

pub fn store_original_and_create_replica(input_file: &Path, job_paths: &JobPaths) -> Result<(), AppError> {
    fs::copy(input_file, &job_paths.original_path)
        .map_err(|e| AppError::Internal(format!("original copy failed: {e}")))?;
    fs::copy(input_file, &job_paths.replica_path)
        .map_err(|e| AppError::Internal(format!("replica copy failed: {e}")))?;
    Ok(())
}
