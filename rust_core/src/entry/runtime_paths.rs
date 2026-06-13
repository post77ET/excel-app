use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::entry::entry_state::JobPaths;
use crate::infra::app_error::AppError;

pub fn ensure_runtime_dirs(project_root: &Path) -> Result<(), AppError> {
    fs::create_dir_all(project_root.join("input"))
        .map_err(|e| AppError::Internal(format!("create input dir failed: {e}")))?;
    fs::create_dir_all(project_root.join("working"))
        .map_err(|e| AppError::Internal(format!("create working dir failed: {e}")))?;
    fs::create_dir_all(resolve_output_root(project_root))
        .map_err(|e| AppError::Internal(format!("create output dir failed: {e}")))?;
    Ok(())
}

pub fn project_root() -> Result<PathBuf, AppError> {
    std::env::current_dir().map_err(|e| AppError::Internal(format!("current_dir failed: {e}")))
}

pub fn build_job_paths(project_root: &Path, input_file: &Path) -> Result<JobPaths, AppError> {
    let file_name = input_file
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| AppError::Internal("input filename resolve failed".to_string()))?;
    let stem = input_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("workbook");

    let job_id = std::env::var("ETB_REQUEST_ID").unwrap_or_else(|_| build_job_id());

    let output_ui_path = if let Ok(explicit_output) = std::env::var("ETB_UI_OUTPUT") {
        let trimmed = explicit_output.trim();
        if trimmed.is_empty() {
            default_output_path(project_root, &job_id, stem)
        } else {
            let path = PathBuf::from(trimmed);
            ensure_parent_dir(&path)?;
            path
        }
    } else {
        default_output_path(project_root, &job_id, stem)
    };

    Ok(JobPaths {
        job_id: job_id.clone(),
        original_path: project_root.join("input").join(format!("{}_{}", job_id, file_name)),
        replica_path: project_root.join("working").join(format!("{}_{}", job_id, file_name)),
        output_ui_path,
    })
}

fn default_output_path(project_root: &Path, job_id: &str, stem: &str) -> PathBuf {
    resolve_output_root(project_root).join(format!("{}_{}_ui.xlsx", job_id, stem))
}

fn resolve_output_root(project_root: &Path) -> PathBuf {
    std::env::var("ETB_OUTPUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| project_root.join("output"))
}

fn ensure_parent_dir(path: &Path) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|e| AppError::Internal(format!("create output parent dir failed: {e}")))?;
        }
    }
    Ok(())
}

fn build_job_id() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("job{seconds}")
}
