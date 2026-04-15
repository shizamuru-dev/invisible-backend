use crate::models::OutgoingMessage;
use anyhow::Result;
use redis::aio::ConnectionManager;
use sqlx::PgPool;
use std::future::Future;
use std::pin::Pin;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait OfflineMessageRepository: Send + Sync {
    fn save_offline_message(
        &self,
        to_user: &str,
        payload: &serde_json::Value,
    ) -> BoxFuture<'_, Result<()>>;

    fn fetch_and_delete_offline_messages(
        &self,
        user_id: &str,
    ) -> BoxFuture<'_, Result<Vec<OutgoingMessage>>>;
}

pub trait PubSubRepository: Send + Sync {
    fn publish_message(&self, channel: &str, message: &str) -> BoxFuture<'_, Result<i32>>;
}

pub trait PresenceRepository: Send + Sync {
    fn add_connection(&self, user_id: &str, conn_id: &str) -> BoxFuture<'_, Result<bool>>;
    fn remove_connection(&self, user_id: &str, conn_id: &str) -> BoxFuture<'_, Result<bool>>;
    fn is_online(&self, user_id: &str) -> BoxFuture<'_, Result<bool>>;
}

pub struct PgOfflineMessageRepository {
    pool: PgPool,
}

impl PgOfflineMessageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl OfflineMessageRepository for PgOfflineMessageRepository {
    fn save_offline_message(
        &self,
        to_user: &str,
        payload: &serde_json::Value,
    ) -> BoxFuture<'_, Result<()>> {
        let pool = self.pool.clone();
        let to_user = to_user.to_owned();
        let payload = payload.clone();
        Box::pin(async move {
            sqlx::query("INSERT INTO offline_messages (to_user, payload) VALUES ($1, $2)")
                .bind(to_user)
                .bind(payload)
                .execute(&pool)
                .await?;
            Ok(())
        })
    }

    fn fetch_and_delete_offline_messages(
        &self,
        user_id: &str,
    ) -> BoxFuture<'_, Result<Vec<OutgoingMessage>>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_owned();
        Box::pin(async move {
            let mut tx = pool.begin().await?;

            let messages = sqlx::query("SELECT id, payload FROM offline_messages WHERE to_user = $1 ORDER BY created_at ASC")
                .bind(user_id)
                .fetch_all(&mut *tx)
                .await?;

            let mut decoded = Vec::new();
            let mut ids_to_delete = Vec::new();

            for row in messages {
                use sqlx::Row;
                let id: i32 = row.get("id");
                let payload: serde_json::Value = row.get("payload");

                match serde_json::from_value::<OutgoingMessage>(payload) {
                    Ok(msg) => {
                        decoded.push(msg);
                        ids_to_delete.push(id);
                    }
                    Err(e) => {
                        tracing::error!("Failed to decode offline message {}: {}", id, e);
                    }
                }
            }

            if !ids_to_delete.is_empty() {
                sqlx::query("DELETE FROM offline_messages WHERE id = ANY($1)")
                    .bind(&ids_to_delete)
                    .execute(&mut *tx)
                    .await?;
            }

            tx.commit().await?;

            Ok(decoded)
        })
    }
}

pub struct RedisPubSubRepository {
    manager: ConnectionManager,
}

impl RedisPubSubRepository {
    pub fn new(manager: ConnectionManager) -> Self {
        Self { manager }
    }
}

impl PubSubRepository for RedisPubSubRepository {
    fn publish_message(&self, channel: &str, message: &str) -> BoxFuture<'_, Result<i32>> {
        let mut conn = self.manager.clone();
        let channel = channel.to_owned();
        let message = message.to_owned();
        Box::pin(async move {
            let receivers: i32 = redis::cmd("PUBLISH")
                .arg(channel)
                .arg(message)
                .query_async(&mut conn)
                .await?;
            Ok(receivers)
        })
    }
}

pub struct RedisPresenceRepository {
    manager: ConnectionManager,
}

impl RedisPresenceRepository {
    pub fn new(manager: ConnectionManager) -> Self {
        Self { manager }
    }
}

impl PresenceRepository for RedisPresenceRepository {
    fn add_connection(&self, user_id: &str, conn_id: &str) -> BoxFuture<'_, Result<bool>> {
        let mut conn = self.manager.clone();
        let key = format!("presence:{}", user_id);
        let conn_id = conn_id.to_owned();
        Box::pin(async move {
            let added: i64 = redis::cmd("SADD")
                .arg(&key)
                .arg(conn_id)
                .query_async(&mut conn)
                .await?;
            let card: i64 = redis::cmd("SCARD").arg(&key).query_async(&mut conn).await?;
            Ok(added > 0 && card == 1) // true if this is the first connection
        })
    }

    fn remove_connection(&self, user_id: &str, conn_id: &str) -> BoxFuture<'_, Result<bool>> {
        let mut conn = self.manager.clone();
        let key = format!("presence:{}", user_id);
        let conn_id = conn_id.to_owned();
        Box::pin(async move {
            let removed: i64 = redis::cmd("SREM")
                .arg(&key)
                .arg(conn_id)
                .query_async(&mut conn)
                .await?;
            let card: i64 = redis::cmd("SCARD").arg(&key).query_async(&mut conn).await?;
            Ok(removed > 0 && card == 0) // true if this was the last connection
        })
    }

    fn is_online(&self, user_id: &str) -> BoxFuture<'_, Result<bool>> {
        let mut conn = self.manager.clone();
        let key = format!("presence:{}", user_id);
        Box::pin(async move {
            let card: i64 = redis::cmd("SCARD").arg(&key).query_async(&mut conn).await?;
            Ok(card > 0)
        })
    }
}
