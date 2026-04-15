use s3::Bucket;
use s3::Region;
use s3::creds::Credentials;

#[tokio::main]
async fn main() {
    let creds = Credentials::new(Some("minioadmin"), Some("minioadmin"), None, None, None).unwrap();
    let region = Region::Custom {
        region: "us-east-1".to_string(),
        endpoint: "http://127.0.0.1:9000".to_string(),
    };
    let bucket = Bucket::new("uploads", region, creds).unwrap().with_path_style();
    
    let url = bucket.presign_put("/test.jpg", 300, None, None).await.unwrap();
    println!("URL: {}", url);
}
