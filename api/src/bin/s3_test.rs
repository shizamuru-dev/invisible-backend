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
    let bucket = Bucket::new("uploads", region, creds)
        .unwrap()
        .with_path_style();
    let put_url = bucket
        .presign_put("/test.jpg", 3600, None, None)
        .await
        .unwrap();
    let get_url = bucket.presign_get("/test.jpg", 3600, None).await.unwrap();
    println!("Put: {}", put_url);
    println!("Get: {}", get_url);
}
