use axum::http::HeaderMap;
use s3::Bucket;
use s3::Region;
use s3::creds::Credentials;

#[tokio::main]
async fn main() {
    let region = Region::Custom {
        region: "us-east-1".to_string(),
        endpoint: "http://localhost:9000".to_string(),
    };
    let credentials =
        Credentials::new(Some("minioadmin"), Some("minioadmin"), None, None, None).unwrap();
    let bucket = Bucket::new("uploads", region, credentials)
        .unwrap()
        .with_path_style();

    let mut custom_headers = HeaderMap::new();
    custom_headers.insert("content-type", "image/jpeg".parse().unwrap());

    let url = bucket
        .presign_put("/test_with_header.jpg", 3600, Some(custom_headers), None)
        .await
        .unwrap();
    println!("Presigned PUT URL: {}", url);
}
