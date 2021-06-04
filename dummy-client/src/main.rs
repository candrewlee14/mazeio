use anyhow::{anyhow, bail, Context, Result};
use crossterm::cursor::MoveTo;
use crossterm::event::{Event, KeyCode, KeyEvent};
use crossterm::style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor};
use crossterm::{cursor, execute, queue, terminal, ExecutableCommand, QueueableCommand};
use mazeio_shared::{Direction, Maze, Player};
use serde::{Deserialize, Serialize};
use std::io::{stdout, Stdout, Write};
use std::{ascii::AsciiExt, error::Error, net::SocketAddr, sync::Arc};
use tokio::io::{
    self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf,
};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, RwLock};

type AtomicBool = Arc<RwLock<bool>>;
type AtomicPlayers = Arc<RwLock<Vec<Player>>>;
type AtomicStdout = Arc<Mutex<Stdout>>;
type AtomicDirectionOption = Arc<RwLock<Option<Direction>>>;
type AtomicDirectionVec = Arc<RwLock<Vec<Direction>>>;
type AtomicMazeOption = Arc<RwLock<Option<Maze>>>;
type BufferGrid = Vec<Vec<(Color, Color, char)>>;

async fn send_dummy_input(mut write_stream: OwnedWriteHalf) -> Result<()> {
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(25));
    loop {
        interval.tick().await;
        let cur_dir: Direction = rand::random();
        let mut dir_ser = serde_json::to_string(&cur_dir)?;
        dir_ser.push('\n');
        match tokio::time::timeout(
            std::time::Duration::from_millis(15),
            write_stream.write_all(dir_ser.as_bytes()),
        )
        .await
        {
            Ok(Ok(_)) => (),
            Ok(Err(_)) | Err(_) => {
                return Err("Send Failed").map_err(anyhow::Error::msg);
            }
        };
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let server_ip = std::env::args()
        .nth(1)
        .unwrap_or("127.0.0.1:5000".to_string());
    let current_dir: AtomicDirectionVec = Arc::new(RwLock::new(Vec::new()));
    let stream = TcpStream::connect(&server_ip).await?;
    let (read_stream, write_stream) = stream.into_split();
    let stream_as_buf = BufReader::new(read_stream);
    let players: AtomicPlayers = Arc::new(RwLock::new(Vec::new()));
    let send_handle = tokio::spawn(async move { send_dummy_input(write_stream).await });
    for _ in 0..100 {
        let stream = TcpStream::connect(&server_ip).await?;
        let (read_stream, write_stream) = stream.into_split();
        let stream_as_buf = BufReader::new(read_stream);
        tokio::spawn(async move { send_dummy_input(write_stream).await });
    }
    match tokio::try_join!(send_handle) {
        Ok(_) => (),
        Err(e) => println!("Error: {:#?}", e),
    };
    Ok(())
}
