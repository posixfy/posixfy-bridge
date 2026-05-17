use axum::{
    body::Body,
    http::{header, Method, Request, Response},
    middleware::Next,
    Router,
};
use http_body_util::BodyExt;
use posixfy_bridge::api;
use posixfy_bridge::config::AppConfig;
use posixfy_bridge::fs::lock::LockManager;
use posixfy_bridge::fs::watcher::FileWatcher;
use posixfy_bridge::models::mount::MountPoint;
use posixfy_bridge::RequestId;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use tower::ServiceExt;

const TEST_API_KEY: &str = "test-api-key";

fn generate_request_id() -> String {
    format!("{:016x}", rand::random::<u64>())
}

async fn request_id_middleware(req: Request<Body>, next: Next) -> Response<Body> {
    let request_id = req
        .headers()
        .get("X-Request-Id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| generate_request_id());

    let mut req = req;
    req.extensions_mut().insert(RequestId(request_id.clone()));

    let mut resp = next.run(req).await;
    resp.headers_mut().insert(
        "X-Request-Id",
        header::HeaderValue::from_str(&request_id).unwrap(),
    );
    resp
}

/// In-process test helper that builds the FileBridge app with a temporary mount point.
pub struct TestApp {
    pub app: Router,
    pub _tmp: TempDir,
    pub mount_root: PathBuf,
}

impl TestApp {
    pub fn new() -> Self {
        let tmp = tempfile::tempdir().expect("failed to create temp dir");
        let mount_root = tmp.path().join("data");
        fs::create_dir_all(&mount_root).expect("failed to create mount root");

        let config = AppConfig {
            api_key: TEST_API_KEY.to_string(),
            listen_addr: "127.0.0.1:0".to_string(),
            mount_points_raw: format!("test:{}", mount_root.to_string_lossy()),
            cors_origins: Vec::new(),
        };

        let mount_points = MountPoint::parse_all(&config.mount_points_raw);

        let state = posixfy_bridge::AppState {
            config,
            mount_points,
            lock_manager: LockManager::new(),
            file_watcher: FileWatcher::new(),
        };

        let app = Router::new()
            .route("/health", axum::routing::get(|| async { "ok" }))
            .merge(api::routes())
            .with_state(state)
            .layer(axum::middleware::from_fn(request_id_middleware));

        Self {
            app,
            _tmp: tmp,
            mount_root,
        }
    }

    /// Send a GET request with auth
    pub async fn get(&mut self, path: &str, query: &str) -> Response<Body> {
        let uri = if query.is_empty() {
            path.to_string()
        } else {
            format!("{}?{}", path, query)
        };
        let request = Request::builder()
            .method(Method::GET)
            .uri(&uri)
            .header("X-API-Key", TEST_API_KEY)
            .header("X-FS-UID", "1000")
            .header("X-FS-GID", "1000")
            .body(Body::empty())
            .unwrap();
        self.app.clone().oneshot(request).await.unwrap()
    }

    /// Send a GET request without auth
    pub async fn get_unauthed(&mut self, path: &str) -> Response<Body> {
        let request = Request::builder()
            .method(Method::GET)
            .uri(path)
            .body(Body::empty())
            .unwrap();
        self.app.clone().oneshot(request).await.unwrap()
    }

    /// Send a POST request with JSON body and auth
    pub async fn post_json(&mut self, path: &str, body: &serde_json::Value) -> Response<Body> {
        let request = Request::builder()
            .method(Method::POST)
            .uri(path)
            .header("X-API-Key", TEST_API_KEY)
            .header("X-FS-UID", "1000")
            .header("X-FS-GID", "1000")
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        self.app.clone().oneshot(request).await.unwrap()
    }

    /// Send a POST request (empty body) with auth
    pub async fn post(&mut self, path: &str, query: &str) -> Response<Body> {
        let uri = if query.is_empty() {
            path.to_string()
        } else {
            format!("{}?{}", path, query)
        };
        let request = Request::builder()
            .method(Method::POST)
            .uri(&uri)
            .header("X-API-Key", TEST_API_KEY)
            .header("X-FS-UID", "1000")
            .header("X-FS-GID", "1000")
            .body(Body::empty())
            .unwrap();
        self.app.clone().oneshot(request).await.unwrap()
    }

    /// Send a POST request with multipart body and auth
    pub async fn post_multipart(
        &mut self,
        path: &str,
        query: &str,
        parts: Vec<(&str, Vec<u8>)>,
    ) -> Response<Body> {
        let boundary = "----TestBoundary123456";
        let mut body_bytes = Vec::new();
        for (name, data) in &parts {
            body_bytes.extend_from_slice(b"--");
            body_bytes.extend_from_slice(boundary.as_bytes());
            body_bytes.extend_from_slice(b"\r\n");
            body_bytes.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\n",
                    name
                )
                .as_bytes(),
            );
            body_bytes.extend_from_slice(b"\r\n");
            body_bytes.extend_from_slice(data);
            body_bytes.extend_from_slice(b"\r\n");
        }
        body_bytes.extend_from_slice(b"--");
        body_bytes.extend_from_slice(boundary.as_bytes());
        body_bytes.extend_from_slice(b"--\r\n");

        let uri = if query.is_empty() {
            path.to_string()
        } else {
            format!("{}?{}", path, query)
        };
        let request = Request::builder()
            .method(Method::POST)
            .uri(&uri)
            .header("X-API-Key", TEST_API_KEY)
            .header("X-FS-UID", "1000")
            .header("X-FS-GID", "1000")
            .header(
                "Content-Type",
                format!("multipart/form-data; boundary={}", boundary),
            )
            .body(Body::from(body_bytes))
            .unwrap();
        self.app.clone().oneshot(request).await.unwrap()
    }

    /// Send a DELETE request with auth
    pub async fn delete(&mut self, path: &str, query: &str) -> Response<Body> {
        let uri = if query.is_empty() {
            path.to_string()
        } else {
            format!("{}?{}", path, query)
        };
        let request = Request::builder()
            .method(Method::DELETE)
            .uri(&uri)
            .header("X-API-Key", TEST_API_KEY)
            .header("X-FS-UID", "1000")
            .header("X-FS-GID", "1000")
            .body(Body::empty())
            .unwrap();
        self.app.clone().oneshot(request).await.unwrap()
    }

    /// Extract the body as a string
    pub async fn body_string(response: Response<Body>) -> String {
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        String::from_utf8_lossy(&bytes).to_string()
    }

    /// Extract the body as bytes
    pub async fn body_bytes(response: Response<Body>) -> Vec<u8> {
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec()
    }
}
