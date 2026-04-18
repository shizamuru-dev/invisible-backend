use figment::{Figment, providers::{Env, Serialized}};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Config {
    s3_bucket: String,
}

fn main() {
    std::env::set_var("S3_BUCKET", "test_bucket");
    let c: Config = Figment::from(Serialized::defaults(Config { s3_bucket: "default_bucket".into() }))
        .merge(Env::raw())
        .extract()
        .unwrap();
    println!("s3_bucket={}", c.s3_bucket);
}
