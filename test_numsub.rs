use redis::AsyncCommands;
#[tokio::main]
async fn main() {
    let client = redis::Client::open("redis://127.0.0.1/").unwrap();
    let mut conn = client.get_multiplexed_tokio_connection().await.unwrap();
    let res: std::collections::HashMap<String, usize> = redis::cmd("PUBSUB").arg("NUMSUB").arg("user:1").query_async(&mut conn).await.unwrap();
    println!("{:?}", res);
}
