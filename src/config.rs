use std::env;

#[derive(Clone)]
pub struct AppConfig {
    pub api_key: String,
    pub listen_addr: String,
    pub mount_points_raw: String,
    pub cors_origins: Vec<String>,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let cors_raw = env::var("CORS_ORIGINS").unwrap_or_default();
        let cors_origins: Vec<String> = cors_raw
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Self {
            api_key: env::var("API_KEY").expect("API_KEY must be set"),
            listen_addr: env::var("LISTEN_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_string()),
            mount_points_raw: env::var("MOUNT_POINTS").unwrap_or_default(),
            cors_origins,
        }
    }
}
