use axum::http::HeaderMap;
use s3::Bucket;
use s3::creds::Credentials;
use s3::region::Region;

#[tokio::main]
async fn main() {
    let region = Region::Custom {
        region: "us-east-1".to_string(),
        endpoint: "http://localhost:9000".to_string(),
    };

    let creds = Credentials::new(Some("minioadmin"), Some("minioadmin"), None, None, None).unwrap();

    let bucket = Bucket::new("test", region, creds)
        .unwrap()
        .with_path_style();

    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        "image/png".parse().unwrap(),
    );

    let url = bucket
        .presign_put("/test.png", 3600, Some(headers.clone()), None)
        .await
        .unwrap();
    println!("URL with custom_headers Content-Type: {}", url);

    // what if we add it to the bucket instead?
    let mut bucket_with_headers = bucket.clone();
    bucket_with_headers.add_header("Content-Type", "image/png");
    let url2 = bucket_with_headers
        .presign_put("/test.png", 3600, None, None)
        .await
        .unwrap();
    println!("URL with bucket headers: {}", url2);
}
