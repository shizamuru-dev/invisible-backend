use s3::{Bucket, Region};
use s3::creds::Credentials;
fn main() {
    let region = Region::Custom {
        region: "us-east-1".to_string(),
        endpoint: "http://localhost:9000".to_string(),
    };
    let credentials = Credentials::new(Some("minioadmin"), Some("minioadmin"), None, None, None).unwrap();
    let bucket = Bucket::new("uploads", region, credentials).unwrap();
    let put_url = bucket.presign_put("/test.jpg", 3600, None).unwrap();
    let get_url = bucket.presign_get("/test.jpg", 3600, None).unwrap();
    println!("Put: {}", put_url);
}
