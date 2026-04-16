use figment::{
    Figment,
    providers::{Env, Serialized},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub redis_url: String,
    pub jwt_secret: String,
    pub api_url: String,
    pub api_host: String,
    pub api_port: u16,

    // Logging
    pub loki_url: Option<String>,

    pub s3_endpoint: String,
    pub s3_access_key: String,
    pub s3_secret_key: String,
    pub s3_region: String,
    pub s3_bucket: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            database_url: "postgres://invisible:password@127.0.0.1:5432/invisible_chat".into(),
            redis_url: "redis://127.0.0.1:6379/".into(),
            jwt_secret: "super-secret-key-for-dev".into(),
            api_url: "http://localhost:3001".into(),
            api_host: "0.0.0.0".into(),
            api_port: 3001,
            loki_url: Some("http://127.0.0.1:3100".into()),
            s3_endpoint: "http://localhost:9000".into(),
            s3_access_key: "minioadmin".into(),
            s3_secret_key: "minioadmin".into(),
            s3_region: "us-east-1".into(),
            s3_bucket: "uploads".into(),
        }
    }
}

impl AppConfig {
    pub fn load() -> Result<Self, Box<figment::Error>> {
        let _ = dotenvy::dotenv();

        Figment::from(Serialized::defaults(AppConfig::default()))
            .merge(Env::raw()) // e.g. JWT_SECRET, DATABASE_URL directly
            .extract()
            .map_err(Box::new)
    }
}
