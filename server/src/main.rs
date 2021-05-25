use anyhow::Result;
use mazeio_shared::{move_in_dir, Direction, Maze, Player};
use std::sync::Arc;
use std::{collections::HashMap, net::SocketAddr};
use tokio::io::{
    self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf,
};
use tokio::net::{
    tcp::{OwnedReadHalf, OwnedWriteHalf},
    TcpListener, TcpStream,
};
use tokio::sync::{Mutex, RwLock};
use tokio::time::{interval, Duration};
use tracing::{event, instrument, Level};
use tracing_subscriber;

type AtomicReadStream = Arc<Mutex<BufReader<OwnedReadHalf>>>;
type AtomicWriteStream = Arc<Mutex<OwnedWriteHalf>>;
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
            event!(Level::TRACE, "Read-locked player for serializing");
            let readable_player = player_lock.read().await;
            let player_str = (*readable_player).to_json()?;
            event!(Level::TRACE, "Read-unlocked player after serializing");
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
    let mut interval = interval(Duration::from_millis(50));
    loop {
        interval.tick().await;
        let players_serialized = atomic_hashmap_to_string(players.clone()).await?;
        let readable_write_streams = write_streams.read().await;
        event!(Level::TRACE, "Writing to players, lock obtained");
        for (i, stream) in readable_write_streams.iter().enumerate() {
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
    width: usize,
    height: usize,
) -> Result<()> {
    let mut interval = interval(Duration::from_millis(50));
    loop {
        interval.tick().await;
        event!(Level::TRACE, "Reading Streams");
        let readable_streams = read_streams.read().await;
        for stream in readable_streams.iter() {
            let mut editable_stream = stream.lock().await;
            let mut recv = String::new();
            match tokio::time::timeout(
                Duration::from_millis(25),
                (*editable_stream).read_line(&mut recv),
            )
            .await
            {
                Ok(Ok(0)) => {}
                Ok(Ok(_)) => {
                    event!(Level::DEBUG, "Client sent data: {}", recv);
                    let players_readable = players.read().await;
                    let player_arc: &AtomicPlayer = players_readable
                        .get(&(*editable_stream).get_ref().as_ref().peer_addr()?)
                        .unwrap();
                    let mut player_writeable = player_arc.write().await;
                    let dir: Direction = serde_json::from_str(&recv)?;
                    let mut x = (*player_writeable).x;
                    let mut y = (*player_writeable).y;
                    move_in_dir(&mut x, &mut y, 1, 1, width - 2, height - 2, &dir, 1);
                    (*player_writeable).x = x;
                    (*player_writeable).y = y;
                }
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

#[instrument(skip(listener, read_streams, write_streams, players, maze_arc))]
async fn accept_connections(
    listener: TcpListener,
    read_streams: AtomicVec<AtomicReadStream>,
    write_streams: AtomicVec<AtomicWriteStream>,
    players: AtomicHashMap<SocketAddr, AtomicPlayer>,
    maze_arc: Arc<Maze>,
) -> Result<()> {
    loop {
        event!(Level::TRACE, "Looking for new connections");
        match listener.accept().await {
            Ok((mut stream, addr)) => {
                event!(Level::INFO, "Client connected at address {:?}", addr);
                let mut ser_maze = serde_json::to_string(&*maze_arc)?;
                ser_maze.push('\n');
                stream.write_all(&ser_maze.as_bytes()).await?;
                let (read_stream, write_stream) = stream.into_split();
                {
                    event!(Level::TRACE, "Waiting for access to for write_stream write");
                    let mut mutable_write_streams = write_streams.write().await;
                    (*mutable_write_streams).push(Arc::new(Mutex::new(write_stream)));
                    event!(Level::TRACE, "Stream write complete");
                    event!(Level::TRACE, "Waiting for access to for read_stream write");
                    let mut mutable_read_streams = read_streams.write().await;
                    event!(Level::TRACE, "Obtained lock for read_stream write");
                    (*mutable_read_streams).push(Arc::new(Mutex::new(BufReader::new(read_stream))));
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

    let maze_width = 15;
    let maze_height = 10;
    let maze_arc = Arc::new(mazeio_shared::Maze::new(maze_width, maze_height));

    event!(Level::INFO, "Server started!");
    let listener = TcpListener::bind("127.0.0.1:5000").await?;
    let write_streams: AtomicVec<AtomicWriteStream> = Arc::new(RwLock::new(Vec::new()));
    let read_streams: AtomicVec<AtomicReadStream> = Arc::new(RwLock::new(Vec::new()));
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
            accept_connections(
                listener,
                read_streams_arc,
                write_streams_arc,
                players_arc,
                maze_arc,
            )
            .await
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
            read_clients(
                read_streams_arc,
                write_streams_arc,
                players_arc,
                maze_width,
                maze_height,
            )
            .await
        })
    };
    match tokio::try_join!(connection_thread, process_thread, read_thread) {
        Ok(_) => (),
        Err(e) => event!(Level::ERROR, "Error: {:#?}", e),
    };
    Ok(())
}
