use shared::config::AppConfig;
fn main() {
    unsafe {
        std::env::set_var("DATABASE_URL", "test_db");
        std::env::set_var("API_PORT", "1234");
        std::env::set_var("S3_BUCKET", "my_test_bucket");
    }
    let c = AppConfig::load().unwrap();
    println!("s3_bucket={}", c.s3_bucket);
}
