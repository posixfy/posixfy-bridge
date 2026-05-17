use axum::{
    extract::{Query, State},
    response::sse::{Event, Sse},
    routing::get,
    Router,
};
use futures_util::stream::Stream;
use serde::Deserialize;
use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_stream::wrappers::BroadcastStream;

use crate::auth::middleware::AuthApiKeyOnly;
use crate::error::AppError;
use crate::fs::path;
use crate::fs::watcher::FileWatcher;
use crate::AppState;

#[derive(Deserialize)]
pub struct WatchQuery {
    pub mount: String,
    pub path: Option<String>,
}

/// SSE endpoint: GET /api/fs/watch?mount=X&path=/Y
async fn watch(
    State(state): State<AppState>,
    _auth: AuthApiKeyOnly,
    Query(q): Query<WatchQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    let rel_path = q.path.as_deref().unwrap_or("/");
    let resolved = path::resolve_path(&state.mount_points, &q.mount, rel_path)
        .map_err(AppError::BadRequest)?;

    let rx = state.file_watcher.subscribe(resolved.clone());
    let watcher = state.file_watcher.clone();

    let stream = WatchStream {
        inner: BroadcastStream::new(rx),
        dir: resolved,
        watcher,
        keepalive: Box::pin(tokio::time::interval(std::time::Duration::from_secs(15))),
    };

    Ok(Sse::new(stream))
}

struct WatchStream {
    inner: BroadcastStream<crate::fs::watcher::FsEvent>,
    dir: std::path::PathBuf,
    watcher: FileWatcher,
    keepalive: Pin<Box<tokio::time::Interval>>,
}

impl Drop for WatchStream {
    fn drop(&mut self) {
        self.watcher.unsubscribe(&self.dir);
    }
}

impl Stream for WatchStream {
    type Item = Result<Event, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Check for file events
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(event))) => {
                let data = serde_json::to_string(&event).unwrap_or_default();
                return Poll::Ready(Some(Ok(Event::default().event("change").data(data))));
            }
            Poll::Ready(Some(Err(_))) => {
                // Lagged behind, continue
            }
            Poll::Ready(None) => {
                return Poll::Ready(None);
            }
            Poll::Pending => {}
        }

        // Keepalive ping
        match self.keepalive.as_mut().poll_tick(cx) {
            Poll::Ready(_) => Poll::Ready(Some(Ok(Event::default().comment("keepalive")))),
            Poll::Pending => Poll::Pending,
        }
    }
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/fs/watch", get(watch))
}
