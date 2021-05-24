use anyhow::{anyhow, bail, Context, Result};
use mazeio_shared::Player;
use serde::{Deserialize, Serialize};
use std::{ascii::AsciiExt, error::Error, net::SocketAddr, sync::Arc};
use tokio::io::BufReader;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() -> Result<()> {
    let stream = TcpStream::connect("127.0.0.1:5000").await?;
    // This is still writable to as well
    let mut stream_as_buf = BufReader::new(stream);
    println!("Connected to server");
    let mut input = String::new();
    let mut players: Vec<Player> = Vec::new();
    loop {
        match stream_as_buf.read_line(&mut input).await {
            Ok(0) => {}
            Ok(_bytes) => {
                if let Ok(deser_players) = serde_json::from_str(&input.trim()) {
                    players = deser_players;
                } else {
                    println!("{}", input.trim());
                }
                println!("{:?}", players);
                input.clear();
            }
            _ => eprintln!("Read Error"),
        }
    }
}
