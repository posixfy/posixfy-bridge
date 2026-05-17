use axum::{
    body::Body,
    extract::{Multipart, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::Response,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;

use crate::auth::middleware::AuthApiKey;
use crate::error::AppError;
use crate::fs::lock::LockType;
use crate::fs::{operations, path};
use crate::AppState;

const MAX_FILENAME_LEN: usize = 255;
const MAX_PAGE_LIMIT: usize = 1000;

#[derive(Deserialize)]
pub struct FsQuery {
    pub mount: String,
    pub path: Option<String>,
    #[serde(default)]
    pub page: Option<usize>,
    #[serde(default)]
    pub limit: Option<usize>,
}

async fn list(
    State(state): State<AppState>,
    auth: AuthApiKey,
    Query(q): Query<FsQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let rel_path = q.path.as_deref().unwrap_or("/");
    let resolved = path::resolve_path(&state.mount_points, &q.mount, rel_path)
        .map_err(AppError::BadRequest)?;

    let mut entries = operations::list_dir(&resolved, auth.uid, auth.gid, &auth.groups)
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to list directory: {e}")))?;

    let total = entries.len();
    let page = q.page.unwrap_or(1).max(1);
    let limit = q.limit.unwrap_or(100).clamp(1, MAX_PAGE_LIMIT);
    let start = (page - 1) * limit;
    let has_more = start + limit < total;
    let page_entries: Vec<_> = entries.drain(start.min(total)..).take(limit).collect();

    Ok(Json(json!({
        "entries": page_entries,
        "total": total,
        "page": page,
        "limit": limit,
        "has_more": has_more,
    })))
}

/// Parse a single byte range from the Range header.
/// Returns Some((start, end)) where end is inclusive.
fn parse_range_header(range: &str, file_size: u64) -> Option<(u64, u64)> {
    let range = range.strip_prefix("bytes=")?;
    let parts: Vec<&str> = range.splitn(2, '-').collect();
    if parts.len() != 2 {
        return None;
    }

    let start = parts[0].parse::<u64>().ok()?;
    let end = if parts[1].is_empty() {
        file_size - 1
    } else {
        parts[1].parse::<u64>().ok()?
    };

    if start > end || start >= file_size {
        return None; // Will return 416
    }

    Some((start, end.min(file_size - 1)))
}

async fn download(
    State(state): State<AppState>,
    auth: AuthApiKey,
    Query(q): Query<FsQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let rel_path = q.path.as_deref().unwrap_or("/");
    let resolved = path::resolve_path(&state.mount_points, &q.mount, rel_path)
        .map_err(AppError::BadRequest)?;

    let _guard = state
        .lock_manager
        .acquire(&resolved, auth.uid, LockType::Read)
        .map_err(AppError::Conflict)?;

    let data = operations::read_file(&resolved, auth.uid, auth.gid, &auth.groups)
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read file: {e}")))?;

    drop(_guard);

    let filename = resolved
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".to_string());

    let file_size = data.len() as u64;

    // Check for Range header
    if let Some(range_header) = headers.get(header::RANGE).and_then(|v| v.to_str().ok()) {
        if let Some((start, end)) = parse_range_header(range_header, file_size) {
            let chunk = &data[start as usize..=end as usize];
            let content_length = end - start + 1;

            return Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .header(
                    header::CONTENT_RANGE,
                    format!("bytes {}-{}/{}", start, end, file_size),
                )
                .header(header::CONTENT_LENGTH, content_length)
                .header(
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{}\"", filename),
                )
                .body(Body::from(chunk.to_vec()))
                .map_err(|e| AppError::Internal(e.to_string()));
        } else {
            // Range not satisfiable
            return Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::CONTENT_RANGE, format!("bytes */{}", file_size))
                .body(Body::empty())
                .map_err(|e| AppError::Internal(e.to_string()));
        }
    }

    // No Range header — return full file
    let response = Response::builder()
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        )
        .body(Body::from(data))
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(response)
}

/// Sanitize a filename from multipart upload:
/// - Extract only the base filename (strip directory components)
/// - Reject if filename exceeds MAX_FILENAME_LEN
fn sanitize_filename(filename: &str) -> Result<String, String> {
    // Extract base filename using path resolution
    let base = std::path::Path::new(filename)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid filename")?;

    if base.is_empty() {
        return Err("Empty filename".to_string());
    }

    if base.len() > MAX_FILENAME_LEN {
        return Err(format!(
            "Filename too long (max {} bytes)",
            MAX_FILENAME_LEN
        ));
    }

    Ok(base.to_string())
}

async fn upload(
    State(state): State<AppState>,
    auth: AuthApiKey,
    Query(q): Query<FsQuery>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, AppError> {
    let expected_mtime: Option<i64> = headers
        .get("X-Expected-MTime")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok());
    let expected_size: Option<u64> = headers
        .get("X-Expected-Size")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok());

    let mut uploaded = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        let raw_filename = field
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "unnamed".to_string());

        let filename = sanitize_filename(&raw_filename).map_err(AppError::BadRequest)?;

        let data = field
            .bytes()
            .await
            .map_err(|e| AppError::BadRequest(e.to_string()))?;

        let rel = format!("{}/{}", q.path.as_deref().unwrap_or(""), &filename);

        let resolved = path::resolve_new_path(&state.mount_points, &q.mount, &rel)
            .map_err(AppError::BadRequest)?;

        let mut guard = state
            .lock_manager
            .acquire(&resolved, auth.uid, LockType::Write)
            .map_err(AppError::Conflict)?;

        // OCC check: if expected mtime/size are provided, verify they match current file
        if expected_mtime.is_some() || expected_size.is_some() {
            let stat = operations::stat_file(&resolved, auth.uid, auth.gid, &auth.groups).await?;

            if let Some((current_size, current_mtime)) = stat {
                if let Some(exp_mtime) = expected_mtime {
                    if current_mtime != exp_mtime {
                        return Err(AppError::Conflict(
                            "file has been modified since last read (mtime mismatch)".to_string(),
                        ));
                    }
                }
                if let Some(exp_size) = expected_size {
                    if current_size != exp_size {
                        return Err(AppError::Conflict(
                            "file has been modified since last read (size mismatch)".to_string(),
                        ));
                    }
                }
            }
        }

        operations::write_file(&resolved, data.to_vec(), auth.uid, auth.gid, &auth.groups)
            .await
            .map_err(|e| AppError::BadRequest(format!("Failed to write file: {e}")))?;

        guard.release();

        uploaded.push(filename);
    }

    Ok(Json(json!({ "uploaded": uploaded })))
}

async fn delete_file(
    State(state): State<AppState>,
    auth: AuthApiKey,
    Query(q): Query<FsQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let rel_path = q
        .path
        .as_deref()
        .ok_or(AppError::BadRequest("path is required".to_string()))?;
    let resolved = path::resolve_path(&state.mount_points, &q.mount, rel_path)
        .map_err(AppError::BadRequest)?;

    let _guard = state
        .lock_manager
        .acquire(&resolved, auth.uid, LockType::Write)
        .map_err(AppError::Conflict)?;

    operations::delete_path(&resolved, auth.uid, auth.gid, &auth.groups)
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to delete: {e}")))?;

    Ok(Json(json!({ "deleted": true })))
}

async fn mkdir(
    State(state): State<AppState>,
    auth: AuthApiKey,
    Query(q): Query<FsQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let rel_path = q
        .path
        .as_deref()
        .ok_or(AppError::BadRequest("path is required".to_string()))?;
    let resolved = path::resolve_new_path(&state.mount_points, &q.mount, rel_path)
        .map_err(AppError::BadRequest)?;

    operations::create_dir(&resolved, auth.uid, auth.gid, &auth.groups)
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to create directory: {e}")))?;

    Ok(Json(json!({ "created": true })))
}

#[derive(Deserialize)]
pub struct RenameBody {
    pub mount: String,
    pub from: String,
    pub to: String,
}

async fn rename(
    State(state): State<AppState>,
    auth: AuthApiKey,
    Query(q): Query<FsQuery>,
    Json(body): Json<RenameBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Resolve source path (must exist)
    let resolved_from = path::resolve_path(&state.mount_points, &body.mount, &body.from)
        .map_err(AppError::BadRequest)?;

    // Resolve target path (parent must exist, target may not)
    let resolved_to = path::resolve_new_path(&state.mount_points, &body.mount, &body.to)
        .map_err(AppError::BadRequest)?;

    // Verify same mount
    if body.mount != q.mount {
        return Err(AppError::BadRequest(
            "source and target mount must match".to_string(),
        ));
    }

    // Acquire write lock on source
    let _guard = state
        .lock_manager
        .acquire(&resolved_from, auth.uid, LockType::Write)
        .map_err(AppError::Conflict)?;

    operations::rename_path(
        &resolved_from,
        &resolved_to,
        auth.uid,
        auth.gid,
        &auth.groups,
    )
    .await
    .map_err(|e| AppError::BadRequest(format!("Failed to rename: {e}")))?;

    Ok(Json(json!({ "renamed": true })))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/fs/list", get(list))
        .route("/api/fs/file", get(download))
        .route("/api/fs/upload", post(upload))
        .route("/api/fs/delete", delete(delete_file))
        .route("/api/fs/mkdir", post(mkdir))
        .route("/api/fs/rename", post(rename))
}
