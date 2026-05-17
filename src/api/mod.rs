pub mod fs;
pub mod mounts;
pub mod watch;

use axum::Router;

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .merge(mounts::routes())
        .merge(fs::routes())
        .merge(watch::routes())
}
