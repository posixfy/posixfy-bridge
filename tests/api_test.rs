mod common;

use common::TestApp;
use http::StatusCode;
use std::fs;
use tower::ServiceExt;

// ===== Health & Auth =====

#[tokio::test]
async fn health_check_returns_ok() {
    let mut app = TestApp::new();
    let resp = app.get("/health", "").await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn request_without_api_key_returns_401() {
    let mut app = TestApp::new();
    let resp = app.get_unauthed("/api/mounts").await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn request_with_valid_api_key_returns_200() {
    let mut app = TestApp::new();
    let resp = app.get("/api/mounts", "").await;
    assert_eq!(resp.status(), StatusCode::OK);
}

// ===== File Operations Roundtrip =====

#[tokio::test]
async fn upload_then_download_roundtrip() {
    let mut app = TestApp::new();
    fs::create_dir_all(app.mount_root.join("subdir")).unwrap();

    // Upload a file
    let content = b"hello world";
    let resp = app
        .post_multipart(
            "/api/fs/upload",
            "mount=test&path=subdir",
            vec![("test.txt", content.to_vec())],
        )
        .await;
    let status = resp.status();
    let body = TestApp::body_string(resp).await;
    if status != StatusCode::OK {
        eprintln!("Upload failed: {}", body);
    }
    assert_eq!(status, StatusCode::OK);

    // Download and verify
    let resp = app
        .get("/api/fs/file", "mount=test&path=subdir/test.txt")
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = TestApp::body_bytes(resp).await;
    assert_eq!(body, content);
}

#[tokio::test]
async fn list_shows_uploaded_files() {
    let mut app = TestApp::new();
    fs::create_dir_all(app.mount_root.join("listdir")).unwrap();

    // Upload a file
    let resp = app
        .post_multipart(
            "/api/fs/upload",
            "mount=test&path=listdir",
            vec![("myfile.txt", b"content".to_vec())],
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // List directory
    let resp = app.get("/api/fs/list", "mount=test&path=listdir").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = TestApp::body_string(resp).await;
    assert!(body.contains("myfile.txt"));
}

#[tokio::test]
async fn delete_removes_file() {
    let mut app = TestApp::new();
    fs::create_dir_all(app.mount_root.join("del")).unwrap();

    // Upload then delete
    app.post_multipart(
        "/api/fs/upload",
        "mount=test&path=del",
        vec![("todelete.txt", b"data".to_vec())],
    )
    .await;

    let resp = app
        .delete("/api/fs/delete", "mount=test&path=del/todelete.txt")
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify file is gone
    let resp = app
        .get("/api/fs/file", "mount=test&path=del/todelete.txt")
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ===== Filename Sanitization =====

#[tokio::test]
async fn upload_strips_directory_from_filename() {
    let mut app = TestApp::new();
    fs::create_dir_all(app.mount_root.join("san")).unwrap();

    // Upload with a filename that has directory components
    let resp = app
        .post_multipart(
            "/api/fs/upload",
            "mount=test&path=san",
            vec![("../../etc/passwd", b"hacked".to_vec())],
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // File should be at san/passwd, not san/etc/passwd
    let resp = app.get("/api/fs/file", "mount=test&path=san/passwd").await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn upload_rejects_oversized_filename() {
    let mut app = TestApp::new();
    fs::create_dir_all(app.mount_root.join("long")).unwrap();

    let long_name = format!("{}.txt", "a".repeat(256));
    let resp = app
        .post_multipart(
            "/api/fs/upload",
            "mount=test&path=long",
            vec![(&long_name, b"data".to_vec())],
        )
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ===== Pagination =====

#[tokio::test]
async fn list_pagination_returns_correct_slice() {
    let mut app = TestApp::new();
    fs::create_dir_all(app.mount_root.join("pag")).unwrap();

    // Create 15 files
    for i in 0..15 {
        app.post_multipart(
            "/api/fs/upload",
            "mount=test&path=pag",
            vec![(&format!("file{:02}.txt", i), b"x".to_vec())],
        )
        .await;
    }

    // First page: limit=5, page=1
    let resp = app
        .get("/api/fs/list", "mount=test&path=pag&limit=5&page=1")
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&TestApp::body_string(resp).await).unwrap();
    assert_eq!(body["total"], 15);
    assert_eq!(body["page"], 1);
    assert_eq!(body["limit"], 5);
    assert_eq!(body["has_more"], true);
    assert_eq!(body["entries"].as_array().unwrap().len(), 5);

    // Last page: page=3
    let resp = app
        .get("/api/fs/list", "mount=test&path=pag&limit=5&page=3")
        .await;
    let body: serde_json::Value = serde_json::from_str(&TestApp::body_string(resp).await).unwrap();
    assert_eq!(body["has_more"], false);
    assert_eq!(body["entries"].as_array().unwrap().len(), 5);
}

#[tokio::test]
async fn list_pagination_limit_capped_at_maximum() {
    let mut app = TestApp::new();
    fs::create_dir_all(app.mount_root.join("cap")).unwrap();

    for i in 0..5 {
        app.post_multipart(
            "/api/fs/upload",
            "mount=test&path=cap",
            vec![(&format!("f{}.txt", i), b"x".to_vec())],
        )
        .await;
    }

    let resp = app
        .get("/api/fs/list", "mount=test&path=cap&limit=10000")
        .await;
    let body: serde_json::Value = serde_json::from_str(&TestApp::body_string(resp).await).unwrap();
    assert_eq!(body["limit"], 1000);
}

#[tokio::test]
async fn list_pagination_empty_directory() {
    let mut app = TestApp::new();
    fs::create_dir_all(app.mount_root.join("empty")).unwrap();

    let resp = app.get("/api/fs/list", "mount=test&path=empty").await;
    let body: serde_json::Value = serde_json::from_str(&TestApp::body_string(resp).await).unwrap();
    assert_eq!(body["total"], 0);
    assert_eq!(body["entries"].as_array().unwrap().len(), 0);
    assert_eq!(body["has_more"], false);
}

// ===== HTTP Range =====

#[tokio::test]
async fn download_range_returns_206() {
    let mut app = TestApp::new();
    fs::create_dir_all(app.mount_root.join("range")).unwrap();

    // Upload a 200-byte file
    let content: Vec<u8> = (0..200).collect();
    app.post_multipart(
        "/api/fs/upload",
        "mount=test&path=range",
        vec![("big.bin", content.clone())],
    )
    .await;

    // Request bytes 100-149
    let request = http::Request::builder()
        .method(http::Method::GET)
        .uri("/api/fs/file?mount=test&path=range/big.bin")
        .header("X-API-Key", "test-api-key")
        .header("X-FS-UID", "1000")
        .header("X-FS-GID", "1000")
        .header("Range", "bytes=100-149")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.app.clone().oneshot(request).await.unwrap();

    assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(resp.headers().get("content-length").unwrap(), "50");
    assert!(resp.headers().contains_key("content-range"));

    let body = TestApp::body_bytes(resp).await;
    assert_eq!(body.len(), 50);
    assert_eq!(body, &content[100..=149]);
}

#[tokio::test]
async fn download_range_beyond_file_returns_416() {
    let mut app = TestApp::new();
    fs::create_dir_all(app.mount_root.join("r416")).unwrap();

    // Upload a 50-byte file
    app.post_multipart(
        "/api/fs/upload",
        "mount=test&path=r416",
        vec![("small.bin", vec![0u8; 50])],
    )
    .await;

    let request = http::Request::builder()
        .method(http::Method::GET)
        .uri("/api/fs/file?mount=test&path=r416/small.bin")
        .header("X-API-Key", "test-api-key")
        .header("X-FS-UID", "1000")
        .header("X-FS-GID", "1000")
        .header("Range", "bytes=9999-10000")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.app.clone().oneshot(request).await.unwrap();

    assert_eq!(resp.status(), StatusCode::RANGE_NOT_SATISFIABLE);
}

#[tokio::test]
async fn download_no_range_returns_full_file() {
    let mut app = TestApp::new();
    fs::create_dir_all(app.mount_root.join("full")).unwrap();

    let content = b"full content here";
    app.post_multipart(
        "/api/fs/upload",
        "mount=test&path=full",
        vec![("full.bin", content.to_vec())],
    )
    .await;

    let resp = app
        .get("/api/fs/file", "mount=test&path=full/full.bin")
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = TestApp::body_bytes(resp).await;
    assert_eq!(body, content);
}

// ===== Rename =====

#[tokio::test]
async fn rename_within_same_directory() {
    let mut app = TestApp::new();
    fs::create_dir_all(app.mount_root.join("rn")).unwrap();

    app.post_multipart(
        "/api/fs/upload",
        "mount=test&path=rn",
        vec![("old.txt", b"hello".to_vec())],
    )
    .await;

    let body = serde_json::json!({
        "mount": "test",
        "from": "rn/old.txt",
        "to": "rn/new.txt"
    });
    let resp = app.post_json("/api/fs/rename?mount=test", &body).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Old path should fail
    let resp = app.get("/api/fs/file", "mount=test&path=rn/old.txt").await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // New path should work
    let resp = app.get("/api/fs/file", "mount=test&path=rn/new.txt").await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn rename_move_to_different_subdirectory() {
    let mut app = TestApp::new();
    fs::create_dir_all(app.mount_root.join("src")).unwrap();
    fs::create_dir_all(app.mount_root.join("dst")).unwrap();

    app.post_multipart(
        "/api/fs/upload",
        "mount=test&path=src",
        vec![("move.txt", b"data".to_vec())],
    )
    .await;

    let body = serde_json::json!({
        "mount": "test",
        "from": "src/move.txt",
        "to": "dst/moved.txt"
    });
    let resp = app.post_json("/api/fs/rename?mount=test", &body).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify file is at new location
    let resp = app
        .get("/api/fs/file", "mount=test&path=dst/moved.txt")
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn rename_nonexistent_source_returns_error() {
    let mut app = TestApp::new();

    let body = serde_json::json!({
        "mount": "test",
        "from": "nonexistent.txt",
        "to": "new.txt"
    });
    let resp = app.post_json("/api/fs/rename?mount=test", &body).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ===== Mkdir =====

#[tokio::test]
async fn mkdir_creates_directory() {
    let mut app = TestApp::new();
    fs::create_dir_all(app.mount_root.join("mkd")).unwrap();

    let resp = app
        .post("/api/fs/mkdir", "mount=test&path=mkd/newdir")
        .await;
    let status = resp.status();
    let body = TestApp::body_string(resp).await;
    if status != StatusCode::OK {
        eprintln!("Mkdir failed: {}", body);
    }
    assert_eq!(status, StatusCode::OK);

    // Verify dir exists
    let resp = app.get("/api/fs/list", "mount=test&path=mkd").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&TestApp::body_string(resp).await).unwrap();
    assert!(body["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["name"] == "newdir"));
}

// ===== Request ID =====

#[tokio::test]
async fn response_includes_generated_request_id() {
    let mut app = TestApp::new();
    let resp = app.get("/health", "").await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(resp.headers().contains_key("X-Request-Id"));
    let id = resp
        .headers()
        .get("X-Request-Id")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(!id.is_empty());
}

#[tokio::test]
async fn response_echoes_client_request_id() {
    let mut app = TestApp::new();
    let request = http::Request::builder()
        .method(http::Method::GET)
        .uri("/health")
        .header("X-API-Key", "test-api-key")
        .header("X-Request-Id", "my-custom-id")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.app.clone().oneshot(request).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let id = resp
        .headers()
        .get("X-Request-Id")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(id, "my-custom-id");
}
