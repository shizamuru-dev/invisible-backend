use figment::{Figment, providers::{Env, Serialized}};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
struct Config {
    s3_bucket: String,
    database_url: String,
}

fn main() {
    std::env::set_var("S3_BUCKET", "test_bucket");
    std::env::set_var("DATABASE_URL", "test_db");
    let c: Config = Figment::from(Serialized::defaults(Config { 
        s3_bucket: "default_bucket".into(),
        database_url: "default_db".into(),
    }))
        .merge(Env::raw())
        .extract()
        .unwrap();
    println!("s3_bucket={}, database_url={}", c.s3_bucket, c.database_url);
}
