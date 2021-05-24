use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::{collections::HashMap, net::SocketAddr};
use tokio::io::{self, AsyncReadExt, AsyncWriteExt, Error};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{RwLock, RwLockWriteGuard};
use tokio::task::JoinHandle;
use tokio::time::{interval, Duration};
use tracing::{event, instrument, span, Level};
use tracing_subscriber;

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Debug)]
struct Player {
    name: String,
    x: usize,
    y: usize,
}
#[allow(dead_code)]
impl Player {
    pub fn new(name: String) -> Self {
        Self { name, x: 0, y: 0 }
    }
}
type AtomicStream = Arc<RwLock<TcpStream>>;
type AtomicPlayer = Arc<RwLock<Player>>;
type AtomicVec<T> = Arc<RwLock<Vec<T>>>;
type AtomicHashMap<K, V> = Arc<RwLock<HashMap<K, V>>>;

#[instrument(skip(players))]
async fn atomic_hashmap_to_string(
    players: AtomicHashMap<SocketAddr, AtomicPlayer>,
) -> Result<String> {
    let mut players_buf = String::new();
    {
        event!(Level::TRACE, "Serializing players");
        let player_reader = players.read().await;
        if player_reader.len() == 0 {
            return Ok("[]".to_string());
        }
        players_buf.push('[');
        for (_key, player_lock) in player_reader.iter() {
            let mut player_str = String::new();
            {
                event!(Level::TRACE, "Read-locked player for serializing");
                let readable_player = player_lock.read().await;
                player_str = serde_json::to_string(&(*readable_player))?;
                event!(Level::TRACE, "Read-unlocked player after serializing");
            }
            players_buf.push_str(&player_str);
            players_buf.push(',');
        }
    }
    players_buf.pop();
    players_buf.push(']');
    players_buf.push('\n');
    event!(Level::TRACE, "Finished deserializing players");
    Ok(players_buf)
}

#[instrument(skip(streams, players))]
async fn process(
    streams: AtomicVec<AtomicStream>,
    players: AtomicHashMap<SocketAddr, AtomicPlayer>,
) -> Result<()> {
    let mut interval = interval(Duration::from_millis(1000));
    loop {
        interval.tick().await;
        let players_serialized = atomic_hashmap_to_string(players.clone()).await?;
        let read_streams = streams.read().await;
        event!(Level::TRACE, "Writing to players");
        for (i, stream) in read_streams.iter().enumerate() {
            event!(Level::TRACE, "Waiting for access to for stream write");
            let mut editable_stream = stream.write().await;
            event!(Level::TRACE, "Obtained lock for stream write");
            match tokio::time::timeout(
                Duration::from_millis(25),
                (*editable_stream).write_all(players_serialized.as_bytes()),
            )
            .await
            {
                Err(_) => {
                    event!(Level::DEBUG, "Client write timed out");
                }
                _ => {}
            };
            event!(Level::DEBUG, "Wrote to client {}", i);
        }
        event!(Level::TRACE, "Finished writing to players");
    }
}

#[instrument(skip(streams, players))]
async fn read_clients(
    streams: AtomicVec<AtomicStream>,
    players: AtomicHashMap<SocketAddr, AtomicPlayer>,
) -> Result<()> {
    let mut interval = interval(Duration::from_millis(250));
    loop {
        interval.tick().await;
        event!(Level::TRACE, "Reading Streams");
        let read_streams = streams.read().await;
        for stream in read_streams.iter() {
            let mut editable_stream = stream.write().await;
            let mut recv = String::new();
            match (*editable_stream).read_to_string(&mut recv).await {
                Ok(_) => {}
                Err(_) => {
                    event!(Level::WARN, "Stream Read Error");
                }
            };
        }
        event!(Level::TRACE, "Finished reading streams");
    }
}

#[instrument(skip(listener, streams, players))]
async fn accept_connections(
    listener: TcpListener,
    streams: AtomicVec<AtomicStream>,
    players: AtomicHashMap<SocketAddr, AtomicPlayer>,
) -> Result<()> {
    let mut interval = interval(Duration::from_millis(250));
    loop {
        interval.tick().await;
        event!(Level::TRACE, "Looking for new connections");
        match listener.accept().await {
            Ok((mut stream, addr)) => {
                event!(Level::INFO, "Client connected at address {:?}", addr);
                stream.write_all(b"yo welcome\n").await?;
                {
                    event!(Level::TRACE, "Waiting for access to for stream write");
                    let mut mutable_streams = streams.write().await;
                    (*mutable_streams).push(Arc::new(RwLock::new(stream)));
                    event!(Level::TRACE, "Stream write complete");
                }
                {
                    event!(Level::TRACE, "Waiting for access to for players write");
                    let mut mutable_map = players.write().await;
                    (*mutable_map).insert(
                        addr,
                        Arc::new(RwLock::new(Player::new("Guest".to_string()))),
                    );
                    event!(Level::TRACE, "Players write complete");
                }
            }
            Err(_e) => event!(Level::ERROR, "Connection Error"),
        }
        event!(Level::TRACE, "Finished looking for new connections");
    }
}

#[tokio::main]
#[instrument]
async fn main() -> Result<()> {
    let format = tracing_subscriber::fmt::format().without_time();
    let subscriber = tracing_subscriber::fmt()
        .event_format(format)
        .with_max_level(Level::TRACE)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    event!(Level::INFO, "Server started!");
    let listener = TcpListener::bind("127.0.0.1:5000").await?;
    let streams: AtomicVec<AtomicStream> = Arc::new(RwLock::new(Vec::new()));
    let players: AtomicHashMap<SocketAddr, AtomicPlayer> = Arc::new(RwLock::new(HashMap::new()));
    let streams_clone = streams.clone();
    let players_clone = players.clone();
    let connection_thread = tokio::spawn(async move {
        let streams_arc = streams_clone.clone();
        let players_arc = players_clone.clone();
        accept_connections(listener, streams_arc, players_arc).await
    });
    let streams_clone2 = streams.clone();
    let players_clone2 = players.clone();
    let process_thread = tokio::spawn(async move {
        let streams_arc = streams_clone2.clone();
        let players_arc = players_clone2.clone();
        process(streams_arc, players_arc).await
    });
    let streams_clone3 = streams.clone();
    let players_clone3 = players.clone();
    let read_thread = tokio::spawn(async move {
        let streams_arc = streams_clone3.clone();
        let players_arc = players_clone3.clone();
        read_clients(streams_arc, players_arc).await
    });
    match tokio::try_join!(connection_thread, process_thread, read_thread) {
        Ok(_) => (),
        Err(e) => println!("Error: {:?}", e),
    };
    Ok(())
}
