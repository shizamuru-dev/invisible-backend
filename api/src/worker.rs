use anyhow::Result;
use redis::{AsyncCommands, FromRedisValue};

use shared::models::DatabaseEvent;
use sqlx::PgPool;
use tracing::{debug, error, info};

pub async fn start_database_worker(db: PgPool, redis_client: redis::Client) {
    info!("Starting database worker for message_events queue using Redis Streams...");

    let mut backoff = 1;
    let stream_name = "message_events";
    let group_name = "db_workers";
    let consumer_name = "worker1";

    loop {
        let mut conn = match redis_client.get_multiplexed_async_connection().await {
            Ok(c) => {
                backoff = 1;
                c
            }
            Err(e) => {
                error!(
                    "Worker failed to connect to Redis: {}. Retrying in {}s...",
                    e, backoff
                );
                tokio::time::sleep(std::time::Duration::from_secs(backoff)).await;
                backoff = std::cmp::min(backoff * 2, 60);
                continue;
            }
        };

        // Create consumer group if it doesn't exist
        let _: redis::RedisResult<()> = redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(stream_name)
            .arg(group_name)
            .arg("0")
            .arg("MKSTREAM")
            .query_async(&mut conn)
            .await;

        let mut read_id = "0";

        loop {
            let opts = redis::streams::StreamReadOptions::default()
                .group(group_name, consumer_name)
                .block(if read_id == ">" { 2000 } else { 0 })
                .count(100);

            let result: redis::RedisResult<redis::streams::StreamReadReply> =
                conn.xread_options(&[stream_name], &[read_id], &opts).await;

            match result {
                Ok(reply) => {
                    let mut processed_any = false;

                    if reply.keys.is_empty() {
                        if read_id == "0" {
                            info!(
                                "Finished processing Pending Entries List (PEL) for {}",
                                consumer_name
                            );
                            read_id = ">";
                        }
                        continue;
                    }
                    for key in reply.keys {
                        if key.key == stream_name {
                            if !key.ids.is_empty() {
                                processed_any = true;
                            }
                            let mut parseable_ids = Vec::new();
                            let mut unparseable_ids = Vec::new();
                            let mut events = Vec::new();

                            for stream_id in key.ids {
                                if let Some(event_json) = stream_id.map.get("event")
                                    && let Ok(json_str) = String::from_redis_value(event_json)
                                {
                                    if let Ok(event) =
                                        serde_json::from_str::<DatabaseEvent>(&json_str)
                                    {
                                        events.push(event);
                                        parseable_ids.push(stream_id.id.clone());
                                    } else {
                                        error!("Failed to parse DatabaseEvent JSON: {}", json_str);
                                        unparseable_ids.push(stream_id.id.clone());
                                    }
                                } else {
                                    unparseable_ids.push(stream_id.id.clone());
                                }
                            }

                            // Always ACK unparseable messages so they don't block PEL
                            if !unparseable_ids.is_empty() {
                                let mut cmd = redis::cmd("XACK");
                                cmd.arg(stream_name).arg(group_name);
                                for id in &unparseable_ids {
                                    cmd.arg(id);
                                }
                                let _: redis::RedisResult<()> = cmd.query_async(&mut conn).await;
                            }

                            if !events.is_empty() {
                                if let Err(e) = process_events_batch(&db, events).await {
                                    error!("Failed to process event batch: {}", e);
                                    // Don't ACK parseable_ids — they will be retried via PEL
                                } else {
                                    let mut cmd = redis::cmd("XACK");
                                    cmd.arg(stream_name).arg(group_name);
                                    for id in &parseable_ids {
                                        cmd.arg(id);
                                    }
                                    let _: redis::RedisResult<()> =
                                        cmd.query_async(&mut conn).await;
                                }
                            }
                        }
                    }

                    if read_id == "0" && !processed_any {
                        info!(
                            "Finished processing Pending Entries List (PEL) for {}",
                            consumer_name
                        );
                        read_id = ">";
                    }
                }
                Err(e) => {
                    error!("Worker failed to read from Redis stream: {}", e);
                    break; // reconnect
                }
            }
        }
    }
}

pub async fn process_events_batch(db: &PgPool, events: Vec<DatabaseEvent>) -> Result<()> {
    debug!("Worker processing batch of {} events", events.len());

    let mut new_messages = Vec::new();
    let mut read_receipts: Vec<(String, String)> = Vec::new();
    let mut encrypted_messages = Vec::new();

    for event in events {
        match event {
            DatabaseEvent::NewMessage {
                id,
                sender,
                recipient,
                message_type,
                content,
                file_name,
                mime_type,
                file_url,
            } => {
                new_messages.push((
                    id,
                    sender,
                    recipient,
                    message_type,
                    content,
                    file_name,
                    mime_type,
                    file_url,
                ));
            }
            DatabaseEvent::NewEncryptedMessage {
                id,
                sender,
                recipient,
                ciphertexts,
            } => {
                encrypted_messages.push((id, sender, recipient, ciphertexts));
            }
            DatabaseEvent::ReadReceipt { message_id, reader } => {
                read_receipts.push((message_id, reader));
            }
        }
    }

    let mut tx = db.begin().await?;

    if !new_messages.is_empty() {
        let mut query_builder = sqlx::QueryBuilder::new(
            "INSERT INTO messages (id, sender_username, recipient_username, message_type, content, file_name, mime_type, file_url) ",
        );

        query_builder.push_values(new_messages, |mut b, (id, sender, recipient, message_type, content, file_name, mime_type, file_url)| {
            b.push_bind(id)
             .push_bind(sender)
             .push_bind(recipient)
             .push_bind(message_type)
             .push_bind(content)
             .push_bind(file_name)
             .push_bind(mime_type)
             .push_bind(file_url);
        });

        query_builder.push(" ON CONFLICT (id) DO NOTHING");
        query_builder.build().execute(&mut *tx).await?;
    }

    if !encrypted_messages.is_empty() {
        // Insert messages first
        let mut msg_builder = sqlx::QueryBuilder::new(
            "INSERT INTO messages (id, sender_username, recipient_username, message_type, content, file_name, mime_type, file_url) ",
        );
        msg_builder.push_values(&encrypted_messages, |mut b, (id, sender, recipient, _)| {
            b.push_bind(id)
                .push_bind(sender)
                .push_bind(recipient)
                .push_bind("encrypted")
                .push_bind(None::<String>)
                .push_bind(None::<String>)
                .push_bind(None::<String>)
                .push_bind(None::<String>);
        });
        msg_builder.push(" ON CONFLICT (id) DO NOTHING");
        msg_builder.build().execute(&mut *tx).await?;

        // Insert ciphertexts
        let mut ciphertexts_data = Vec::new();
        for (id, _, _, ciphertexts) in encrypted_messages {
            for ct in ciphertexts {
                let device_uuid_res = uuid::Uuid::parse_str(&ct.device_id);
                if let Ok(device_uuid) = device_uuid_res {
                    ciphertexts_data.push((
                        uuid::Uuid::new_v4(),
                        id.clone(),
                        device_uuid,
                        ct.ciphertext,
                        ct.signal_type,
                    ));
                } else {
                    error!("Invalid device UUID: {}", ct.device_id);
                }
            }
        }

        if !ciphertexts_data.is_empty() {
            let mut ct_builder = sqlx::QueryBuilder::new(
                "INSERT INTO message_ciphertexts (id, message_id, device_id, ciphertext, signal_type) ",
            );
            ct_builder.push_values(
                ciphertexts_data,
                |mut b, (id, msg_id, device_id, ciphertext, signal_type)| {
                    b.push_bind(id)
                        .push_bind(msg_id)
                        .push_bind(device_id)
                        .push_bind(ciphertext)
                        .push_bind(signal_type);
                },
            );
            ct_builder.build().execute(&mut *tx).await?;
        }
    }

    if !read_receipts.is_empty() {
        let message_ids: Vec<String> = read_receipts.iter().map(|(id, _)| id.clone()).collect();

        #[derive(Debug, sqlx::FromRow)]
        struct MessageSender {
            id: String,
            sender_username: String,
        }

        let messages: Vec<MessageSender> =
            sqlx::query_as("SELECT id, sender_username FROM messages WHERE id = ANY($1)")
                .bind(&message_ids)
                .fetch_all(&mut *tx)
                .await?;

        let message_map: std::collections::HashMap<String, String> = messages
            .into_iter()
            .map(|m| (m.id, m.sender_username))
            .collect();

        for (message_id, reader) in read_receipts {
            if let Some(sender) = message_map.get(&message_id) {
                sqlx::query(
                    r#"
                    INSERT INTO dialog_read_states (user_username, peer_username, last_read_message_id, unread_count, updated_at)
                    VALUES ($1, $2, $3, 0, NOW())
                    ON CONFLICT (user_username, peer_username)
                    DO UPDATE SET last_read_message_id = $3, unread_count = 0, updated_at = NOW()
                    "#
                )
                .bind(&reader)
                .bind(sender)
                .bind(&message_id)
                .execute(&mut *tx)
                .await?;
            }
        }
    }

    tx.commit().await?;
    Ok(())
}
