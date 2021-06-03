use anyhow::Result;
use mazeio_shared::{move_in_dir, CellType, Direction, Maze, Player};
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
use tokio::task::JoinHandle;
use tokio::time::{interval, Duration};
use tracing::{event, instrument, Level};
use tracing_subscriber;

type AtomicReadStream = Arc<Mutex<BufReader<OwnedReadHalf>>>;
type AtomicWriteStream = Arc<Mutex<OwnedWriteHalf>>;
type AtomicPlayer = Arc<RwLock<Player>>;
type AtomicVec<T> = Arc<RwLock<Vec<T>>>;
type AtomicHashMap<K, V> = Arc<RwLock<HashMap<K, V>>>;
type AtomicMaze = Arc<RwLock<Maze>>;

const READ_INTERVAL: u64 = 15;
const SEND_INTERVAL: u64 = 15;
const CLIENT_WRITE_FAIL_TIME_LIMIT: u64 = 3000;
const SEND_TIMEOUT: u64 = 30;
const READ_TIMEOUT: u64 = 30;

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

#[instrument(skip(write_stream, players))]
async fn send_info_to_client(
    write_stream: AtomicWriteStream,
    players: AtomicHashMap<SocketAddr, AtomicPlayer>,
) -> Result<()> {
    let mut interval = interval(Duration::from_millis(SEND_INTERVAL));
    let mut fails = 0;
    let ip_addr = {
        let editable_stream = write_stream.lock().await;
        (*editable_stream).as_ref().peer_addr()?
    };
    loop {
        interval.tick().await;
        let players_serialized = atomic_hashmap_to_string(players.clone()).await?;
        let mut editable_stream = write_stream.lock().await;
        event!(Level::TRACE, "Obtained lock for stream write");
        match tokio::time::timeout(
            Duration::from_millis(SEND_TIMEOUT),
            (*editable_stream).write_all(players_serialized.as_bytes()),
        )
        .await
        {
            Ok(Ok(_)) => {
                fails = 0;
            }
            Ok(Err(_)) => {
                event!(Level::DEBUG, "Client write failed");
                fails += 1;
                if fails > CLIENT_WRITE_FAIL_TIME_LIMIT / SEND_INTERVAL {
                    event!(Level::INFO, "Client at address {} exited game", ip_addr);
                    let mut editable_players = players.write().await;
                    (*editable_players).remove(&ip_addr);
                    return Ok(());
                }
            }
            Err(_) => {
                event!(Level::WARN, "Client write timed out");
            }
        };
        event!(
            Level::TRACE,
            "Wrote to client at address {}",
            (*editable_stream).as_ref().peer_addr()?
        );
    }
}

#[instrument(skip(read_stream, player, maze))]
async fn read_from_client(
    read_stream: AtomicReadStream,
    player: AtomicPlayer,
    maze: AtomicMaze,
) -> Result<()> {
    let mut interval = interval(Duration::from_millis(READ_INTERVAL));
    loop {
        interval.tick().await;
        let mut editable_stream = read_stream.lock().await;
        let mut recv = String::new();
        match tokio::time::timeout(
            Duration::from_millis(READ_TIMEOUT),
            (*editable_stream).read_line(&mut recv),
        )
        .await
        {
            Ok(Ok(0)) => {}
            Ok(Ok(_)) => {
                event!(Level::DEBUG, "Client sent data: {}", recv);
                let mut player_writeable = player.write().await;
                let dir: Direction = serde_json::from_str(&recv)?;
                let mut x = (*player_writeable).x;
                let mut y = (*player_writeable).y;
                let readable_maze = maze.read().await;
                move_in_dir(
                    &mut x,
                    &mut y,
                    1,
                    1,
                    readable_maze.width,
                    readable_maze.height,
                    &dir,
                    1,
                );
                if readable_maze.cells[y][x] == CellType::Open {
                    (*player_writeable).x = x;
                    (*player_writeable).y = y;
                }
            }
            Ok(Err(_)) => {
                event!(Level::WARN, "Client read error")
            }
            Err(_) => {
                event!(Level::TRACE, "Client read timed out")
            }
        };
    }
}

#[instrument(skip(listener, read_streams, write_streams, players, maze))]
async fn accept_connections(
    listener: TcpListener,
    read_streams: AtomicVec<AtomicReadStream>,
    write_streams: AtomicVec<AtomicWriteStream>,
    players: AtomicHashMap<SocketAddr, AtomicPlayer>,
    maze: AtomicMaze,
) -> Result<()> {
    loop {
        event!(Level::TRACE, "Looking for new connections");
        match listener.accept().await {
            Ok((mut stream, addr)) => {
                event!(Level::INFO, "Client connected at address {:?}", addr);
                let mut ser_maze = {
                    let readable_maze = maze.read().await;
                    serde_json::to_string(&*readable_maze)?
                };
                ser_maze.push('\n');
                stream.write_all(&ser_maze.as_bytes()).await?;

                let (read_stream, write_stream) = stream.into_split();
                let new_player_arc = Arc::new(RwLock::new(Player::new("Guest".to_string())));
                let write_stream_arc = Arc::new(Mutex::new(write_stream));
                let read_stream_arc = Arc::new(Mutex::new(BufReader::new(read_stream)));
                {
                    let mut mutable_write_streams = write_streams.write().await;
                    (*mutable_write_streams).push(write_stream_arc.clone());
                    let mut mutable_read_streams = read_streams.write().await;
                    (*mutable_read_streams).push(read_stream_arc.clone());
                    let mut mutable_map = players.write().await;
                    (*mutable_map).insert(addr, new_player_arc.clone());
                }
                let maze_arc = maze.clone();
                tokio::spawn(async move {
                    read_from_client(read_stream_arc, new_player_arc, maze_arc.clone()).await;
                });
                let players_arc = players.clone();
                tokio::spawn(async move {
                    send_info_to_client(write_stream_arc, players_arc.clone()).await;
                });
            }
            Err(_e) => event!(Level::ERROR, "Connection Error"),
        }
        event!(Level::TRACE, "New connection handling finished");
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
    let maze_height = 20;
    let maze = Arc::new(RwLock::new(mazeio_shared::Maze::new(
        maze_width,
        maze_height,
    )));

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
        let maze_clone = maze.clone();
        tokio::spawn(async move {
            let read_streams_arc = read_streams_clone.clone();
            let write_streams_arc = write_streams_clone.clone();
            let players_arc = players_clone.clone();
            let maze_arc = maze_clone.clone();
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
    match tokio::try_join!(connection_thread) {
        Ok(_) => (),
        Err(e) => event!(Level::ERROR, "Error: {:#?}", e),
    };
    Ok(())
}
