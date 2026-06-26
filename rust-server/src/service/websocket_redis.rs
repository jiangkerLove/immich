//! Redis pub/sub drivers for the socket.io adapter.
//!
//! `socketioxide-redis` defaults to RESP3 push notifications (Redis 7+ / Valkey 7+).
//! Many deployments still run Redis 6 or older brokers without `HELLO`; we fall back to RESP2.

use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::sync::{Arc, RwLock};

use futures_util::StreamExt;
use redis::{AsyncCommands, Client, RedisError};
use socketioxide_redis::drivers::redis::RedisError as SocketRedisDriverError;
use socketioxide_redis::drivers::redis::RedisDriver;
use socketioxide_redis::drivers::{ChanItem, Driver, MessageStream};
use tokio::sync::mpsc;

type HandlerMap = HashMap<String, mpsc::Sender<ChanItem>>;

#[derive(Debug)]
pub struct ImmichRedisError(String);

impl fmt::Display for ImmichRedisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ImmichRedisError {}

impl From<RedisError> for ImmichRedisError {
    fn from(value: RedisError) -> Self {
        Self(value.to_string())
    }
}

impl From<SocketRedisDriverError> for ImmichRedisError {
    fn from(value: SocketRedisDriverError) -> Self {
        Self(value.to_string())
    }
}

pub fn is_resp3_unsupported(err: &RedisError) -> bool {
    let message = err.to_string();
    message.contains("RESP3NotSupported") || message.contains("doesn't support HELLO")
}

pub fn redis_url_with_protocol(redis_url: &str, protocol: &str) -> String {
    if redis_url.contains("protocol=resp") {
        let mut url = redis_url.to_string();
        if url.contains("protocol=resp3") {
            url = url.replace("protocol=resp3", &format!("protocol={protocol}"));
        } else if url.contains("protocol=resp2") {
            url = url.replace("protocol=resp2", &format!("protocol={protocol}"));
        }
        url
    } else if redis_url.contains('?') {
        format!("{redis_url}&protocol={protocol}")
    } else {
        format!("{redis_url}?protocol={protocol}")
    }
}

/// Classic RESP2 pub/sub (compatible with Redis 5+).
#[derive(Clone)]
pub struct Resp2RedisDriver {
    handlers: Arc<RwLock<HandlerMap>>,
    pubsub_sink: redis::aio::PubSubSink,
    publish_conn: redis::aio::MultiplexedConnection,
}

impl Resp2RedisDriver {
    pub async fn new(client: &Client) -> Result<Self, RedisError> {
        let handlers: Arc<RwLock<HandlerMap>> = Arc::new(RwLock::new(HashMap::new()));
        let handlers_clone = handlers.clone();

        let pubsub = client.get_async_pubsub().await?;
        let (pubsub_sink, mut pubsub_stream) = pubsub.split();
        let publish_conn = client.get_multiplexed_async_connection().await?;

        tokio::spawn(async move {
            while let Some(msg) = pubsub_stream.next().await {
                let channel: String = match msg.get_channel() {
                    Ok(channel) => channel,
                    Err(err) => {
                        eprintln!("redis pubsub channel parse error: {err}");
                        continue;
                    }
                };
                let payload: Vec<u8> = match msg.get_payload() {
                    Ok(payload) => payload,
                    Err(err) => {
                        eprintln!("redis pubsub payload parse error: {err}");
                        continue;
                    }
                };
                if let Some(tx) = handlers_clone.read().unwrap().get(&channel) {
                    if let Err(err) = tx.try_send((channel, payload)) {
                        eprintln!("redis pubsub channel full: {err}");
                    }
                }
            }
            eprintln!("redis pubsub stream ended");
        });

        Ok(Self {
            handlers,
            pubsub_sink,
            publish_conn,
        })
    }
}

impl Driver for Resp2RedisDriver {
    type Error = ImmichRedisError;

    async fn publish(&self, chan: String, val: Vec<u8>) -> Result<(), Self::Error> {
        self.publish_conn
            .clone()
            .publish::<_, _, redis::Value>(chan, val)
            .await?;
        Ok(())
    }

    async fn subscribe(
        &self,
        chan: String,
        size: usize,
    ) -> Result<MessageStream<ChanItem>, Self::Error> {
        let mut sink = self.pubsub_sink.clone();
        sink.subscribe(chan.as_str()).await?;
        let (tx, rx) = mpsc::channel(size);
        self.handlers.write().unwrap().insert(chan, tx);
        Ok(MessageStream::new(rx))
    }

    async fn unsubscribe(&self, chan: String) -> Result<(), Self::Error> {
        self.handlers.write().unwrap().remove(&chan);
        let mut sink = self.pubsub_sink.clone();
        sink.unsubscribe(chan.as_str()).await?;
        Ok(())
    }

    async fn num_serv(&self, chan: &str) -> Result<u16, Self::Error> {
        let mut conn = self.publish_conn.clone();
        let (_, count): (String, u16) = redis::cmd("PUBSUB")
            .arg("NUMSUB")
            .arg(chan)
            .query_async(&mut conn)
            .await?;
        Ok(count)
    }
}

#[derive(Clone)]
pub enum ImmichRedisDriver {
    Resp3(RedisDriver),
    Resp2(Resp2RedisDriver),
}

impl Driver for ImmichRedisDriver {
    type Error = ImmichRedisError;

    fn publish(
        &self,
        chan: String,
        val: Vec<u8>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        async move {
            match self {
                Self::Resp3(driver) => driver.publish(chan, val).await.map_err(ImmichRedisError::from),
                Self::Resp2(driver) => driver.publish(chan, val).await,
            }
        }
    }

    async fn subscribe(
        &self,
        chan: String,
        size: usize,
    ) -> Result<MessageStream<ChanItem>, Self::Error> {
        match self {
            Self::Resp3(driver) => driver.subscribe(chan, size).await.map_err(ImmichRedisError::from),
            Self::Resp2(driver) => driver.subscribe(chan, size).await,
        }
    }

    async fn unsubscribe(&self, chan: String) -> Result<(), Self::Error> {
        match self {
            Self::Resp3(driver) => driver.unsubscribe(chan).await.map_err(ImmichRedisError::from),
            Self::Resp2(driver) => driver.unsubscribe(chan).await,
        }
    }

    async fn num_serv(&self, chan: &str) -> Result<u16, Self::Error> {
        match self {
            Self::Resp3(driver) => driver.num_serv(chan).await.map_err(ImmichRedisError::from),
            Self::Resp2(driver) => driver.num_serv(chan).await,
        }
    }
}

pub async fn connect_driver(redis_url: &str) -> Result<ImmichRedisDriver, RedisError> {
    let resp3_url = redis_url_with_protocol(redis_url, "resp3");
    let resp3_client = Client::open(resp3_url)?;

    match RedisDriver::new(&resp3_client).await {
        Ok(driver) => {
            println!("WebSocket Redis adapter using RESP3");
            Ok(ImmichRedisDriver::Resp3(driver))
        }
        Err(err) if is_resp3_unsupported(&err) => {
            eprintln!(
                "Redis does not support RESP3 (HELLO); falling back to RESP2 pub/sub: {err}"
            );
            let resp2_url = redis_url_with_protocol(redis_url, "resp2");
            let resp2_client = Client::open(resp2_url)?;
            Ok(ImmichRedisDriver::Resp2(
                Resp2RedisDriver::new(&resp2_client).await?,
            ))
        }
        Err(err) => Err(err),
    }
}
