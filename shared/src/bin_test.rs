use shared::config::AppConfig;
fn main() {
    unsafe {
        std::env::set_var("DATABASE_URL", "test_db");
    }
    unsafe {
        std::env::set_var("API_PORT", "1234");
    }
    let c = AppConfig::load().unwrap();
    println!("{:?}", c);
}
