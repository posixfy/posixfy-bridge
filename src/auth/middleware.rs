use axum::{extract::FromRequestParts, http::request::Parts};

use crate::config::AppConfig;
use crate::error::AppError;

/// Extractor that validates API Key and extracts filesystem identity from headers.
#[derive(Debug, Clone)]
pub struct AuthApiKey {
    pub uid: u32,
    pub gid: u32,
    pub groups: Vec<u32>,
}

impl<S> FromRequestParts<S> for AuthApiKey
where
    S: Send + Sync,
    AppConfig: axum::extract::FromRef<S>,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let config = AppConfig::from_ref(state);

        // Validate API Key
        let api_key = parts
            .headers
            .get("X-API-Key")
            .and_then(|v| v.to_str().ok())
            .ok_or(AppError::Unauthorized)?;

        if api_key != config.api_key {
            return Err(AppError::Unauthorized);
        }

        // Extract UID (required, must be > 0)
        let uid: u32 = parts
            .headers
            .get("X-FS-UID")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| AppError::BadRequest("X-FS-UID header is required".to_string()))?;

        if uid == 0 {
            return Err(AppError::Forbidden);
        }

        // Extract GID (required)
        let gid: u32 = parts
            .headers
            .get("X-FS-GID")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| AppError::BadRequest("X-FS-GID header is required".to_string()))?;

        // Extract supplementary groups (optional, comma-separated)
        let groups: Vec<u32> = parts
            .headers
            .get("X-FS-Groups")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.split(',').filter_map(|s| s.trim().parse().ok()).collect())
            .unwrap_or_default();

        Ok(AuthApiKey { uid, gid, groups })
    }
}

use axum::extract::FromRef;

/// Extractor that only validates API Key (no FS identity needed).
#[derive(Debug, Clone)]
pub struct AuthApiKeyOnly;

impl<S> FromRequestParts<S> for AuthApiKeyOnly
where
    S: Send + Sync,
    AppConfig: FromRef<S>,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let config = AppConfig::from_ref(state);

        let api_key = parts
            .headers
            .get("X-API-Key")
            .and_then(|v| v.to_str().ok())
            .ok_or(AppError::Unauthorized)?;

        if api_key != config.api_key {
            return Err(AppError::Unauthorized);
        }

        Ok(AuthApiKeyOnly)
    }
}
