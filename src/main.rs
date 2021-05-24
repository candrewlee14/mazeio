use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::SocketAddr};
use std::{env, sync::Arc};
use tokio::io::{self, AsyncReadExt, AsyncWriteExt, Error, ReadHalf, WriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio::time::{interval, Duration};
use tracing::{event, instrument, Level};
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
type AtomicStream = Arc<Mutex<TcpStream>>;
type AtomicReadStream = Arc<Mutex<ReadHalf<TcpStream>>>;
type AtomicWriteStream = Arc<Mutex<WriteHalf<TcpStream>>>;
type AtomicPlayer = Arc<RwLock<Player>>;
type AtomicVec<T> = Arc<Mutex<Vec<T>>>;
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

#[instrument(skip(read_streams, write_streams, players))]
async fn process(
    read_streams: AtomicVec<AtomicReadStream>,
    write_streams: AtomicVec<AtomicWriteStream>,
    players: AtomicHashMap<SocketAddr, AtomicPlayer>,
) -> Result<()> {
    let mut interval = interval(Duration::from_millis(1000));
    loop {
        interval.tick().await;
        let players_serialized = atomic_hashmap_to_string(players.clone()).await?;
        let writeable_streams = write_streams.lock().await;
        event!(Level::TRACE, "Writing to players, lock obtained");
        for (i, stream) in writeable_streams.iter().enumerate() {
            event!(Level::TRACE, "Waiting for access to for stream write");
            let mut editable_stream = stream.lock().await;
            event!(Level::TRACE, "Obtained lock for stream write");
            match tokio::time::timeout(
                Duration::from_millis(25),
                (*editable_stream).write_all(players_serialized.as_bytes()),
            )
            .await
            {
                Ok(Ok(_)) => {}
                Ok(Err(_)) => {
                    event!(Level::WARN, "Client write failed");
                }
                Err(_) => {
                    event!(Level::WARN, "Client write timed out");
                }
            };
            event!(Level::TRACE, "Wrote to client {}", i);
        }
        event!(Level::DEBUG, "Finished writing to players");
    }
}

#[instrument(skip(read_streams, write_streams, players))]
async fn read_clients(
    read_streams: AtomicVec<AtomicReadStream>,
    write_streams: AtomicVec<AtomicWriteStream>,
    players: AtomicHashMap<SocketAddr, AtomicPlayer>,
) -> Result<()> {
    let mut interval = interval(Duration::from_millis(250));
    loop {
        interval.tick().await;
        event!(Level::TRACE, "Reading Streams");
        let readable_streams = read_streams.lock().await;
        for stream in readable_streams.iter() {
            let mut editable_stream = stream.lock().await;
            let mut recv = String::new();
            match tokio::time::timeout(
                Duration::from_millis(25),
                (*editable_stream).read_to_string(&mut recv),
            )
            .await
            {
                Ok(Ok(_)) => {}
                Ok(Err(_)) => {
                    event!(Level::WARN, "Client read error")
                }
                Err(_) => {
                    event!(Level::TRACE, "Client read timed out")
                }
            };
        }
        event!(Level::TRACE, "Finished reading streams");
    }
}

#[instrument(skip(listener, read_streams, write_streams, players))]
async fn accept_connections(
    listener: TcpListener,
    read_streams: AtomicVec<AtomicReadStream>,
    write_streams: AtomicVec<AtomicWriteStream>,
    players: AtomicHashMap<SocketAddr, AtomicPlayer>,
) -> Result<()> {
    loop {
        event!(Level::TRACE, "Looking for new connections");
        match listener.accept().await {
            Ok((mut stream, addr)) => {
                event!(Level::INFO, "Client connected at address {:?}", addr);
                stream.write_all(b"welcome!\n").await?;
                let (read_stream, write_stream) = io::split(stream);
                {
                    event!(Level::DEBUG, "Waiting for access to for write_stream write");
                    let mut mutable_write_streams = write_streams.lock().await;
                    (*mutable_write_streams).push(Arc::new(Mutex::new(write_stream)));
                    event!(Level::DEBUG, "Stream write complete");
                    event!(Level::DEBUG, "Waiting for access to for read_stream write");
                    let mut mutable_read_streams = read_streams.lock().await;
                    event!(Level::DEBUG, "Obtained lock for read_stream write");
                    (*mutable_read_streams).push(Arc::new(Mutex::new(read_stream)));
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
    let env_filter = tracing_subscriber::EnvFilter::from_default_env();
    let format = tracing_subscriber::fmt::format().without_time();
    let subscriber = tracing_subscriber::fmt()
        .event_format(format)
        .with_env_filter(env_filter)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    event!(Level::INFO, "Server started!");
    let listener = TcpListener::bind("127.0.0.1:5000").await?;
    let write_streams: AtomicVec<AtomicWriteStream> = Arc::new(Mutex::new(Vec::new()));
    let read_streams: AtomicVec<AtomicReadStream> = Arc::new(Mutex::new(Vec::new()));
    let players: AtomicHashMap<SocketAddr, AtomicPlayer> = Arc::new(RwLock::new(HashMap::new()));
    let connection_thread = {
        // Accept Connections
        let read_streams_clone = read_streams.clone();
        let write_streams_clone = write_streams.clone();
        let players_clone = players.clone();
        tokio::spawn(async move {
            let read_streams_arc = read_streams_clone.clone();
            let write_streams_arc = write_streams_clone.clone();
            let players_arc = players_clone.clone();
            accept_connections(listener, read_streams_arc, write_streams_arc, players_arc).await
        })
    };
    let process_thread = {
        // Process Connections
        let read_streams_clone = read_streams.clone();
        let write_streams_clone = write_streams.clone();
        let players_clone = players.clone();
        tokio::spawn(async move {
            let read_streams_arc = read_streams_clone.clone();
            let write_streams_arc = write_streams_clone.clone();
            let players_arc = players_clone.clone();
            process(read_streams_arc, write_streams_arc, players_arc).await
        })
    };
    let read_thread = {
        // Read from Client Connections
        let read_streams_clone = read_streams.clone();
        let write_streams_clone = write_streams.clone();
        let players_clone = players.clone();
        tokio::spawn(async move {
            let read_streams_arc = read_streams_clone.clone();
            let write_streams_arc = write_streams_clone.clone();
            let players_arc = players_clone.clone();
            read_clients(read_streams_arc, write_streams_arc, players_arc).await
        })
    };
    match tokio::try_join!(connection_thread, process_thread, read_thread) {
        Ok(_) => (),
        Err(e) => println!("Error: {:?}", e),
    };
    Ok(())
}
