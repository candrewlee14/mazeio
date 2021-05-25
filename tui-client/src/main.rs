use anyhow::{anyhow, bail, Context, Result};
use crossterm::cursor::MoveTo;
use crossterm::event::{Event, KeyCode, KeyEvent};
use crossterm::style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor};
use crossterm::{cursor, execute, queue, terminal, ExecutableCommand, QueueableCommand};
use mazeio_shared::{Maze, Player};
use serde::{Deserialize, Serialize};
use std::io::{stdout, Stdout, Write};
use std::{ascii::AsciiExt, error::Error, net::SocketAddr, sync::Arc};
use tokio::io::BufReader;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, RwLock};

type AtomicPlayers = Arc<RwLock<Vec<Player>>>;
type AtomicMaze = Arc<RwLock<Maze>>;

async fn print_maze(stdout: &mut Stdout, maze: &Maze) -> Result<()> {
    for y in 0..maze.cells.len() {
        for (x, cell) in maze.cells[y].iter().enumerate() {
            stdout
                .queue(MoveTo(x as u16, y as u16))?
                .queue(SetBackgroundColor(Color::Black))?
                .queue(SetForegroundColor(Color::White))?
                .queue(Print(cell.to_char()))?;
        }
    }
    stdout.flush()?;
    Ok(())
}

async fn process(
    mut stream_as_buf: BufReader<TcpStream>,
    players: AtomicPlayers,
    maze: AtomicMaze,
) -> Result<()> {
    let mut input = String::new();
    loop {
        match stream_as_buf.read_line(&mut input).await {
            Ok(0) => {}
            Ok(_bytes) => {
                if let Ok(deser_players) = serde_json::from_str(&input.trim()) {
                    let mut players_writeable = players.write().await;
                    *players_writeable = deser_players;
                } else if let Ok(deser_maze) = serde_json::from_str(&input.trim()) {
                    let mut maze_writeable = maze.write().await;
                    *maze_writeable = deser_maze;
                //println!("{}", maze_writeable.to_string());
                } else {
                    //println!("{}", input.trim());
                }
                //println!("{:?}", players);
                input.clear();
            }
            _ => eprintln!("Read Error"),
        }
    }
}
async fn gui(mut stdout: Stdout, players: AtomicPlayers, maze: AtomicMaze) -> Result<()> {
    loop {
        if crossterm::event::poll(std::time::Duration::from_millis(25))? {
            match crossterm::event::read()? {
                Event::Key(keyevent) if keyevent.code == KeyCode::Esc => {
                    stdout
                        .execute(MoveTo(30, 0))?
                        .execute(SetForegroundColor(Color::Green))?
                        .execute(SetBackgroundColor(Color::Black))?
                        .execute(Print("Exiting program"))?;
                    break;
                }
                _ => (),
            }
        } else {
            let maze_readable = maze.read().await;
            print_maze(&mut stdout, &*maze_readable).await?;
            stdout
                .execute(MoveTo((maze_readable.width + 2) as u16, 0))?
                .execute(SetForegroundColor(Color::Green))?
                .execute(SetBackgroundColor(Color::Black))?
                .execute(Print("Exit with escape key"))?;
        }
    }
    terminal::disable_raw_mode()?;
    stdout.execute(terminal::LeaveAlternateScreen)?;
    Err("Exited function early with escape key").map_err(anyhow::Error::msg)
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut stdout = stdout();
    stdout
        .execute(terminal::EnterAlternateScreen)?
        .execute(cursor::Hide)?;
    terminal::enable_raw_mode()?;
    terminal::Clear(terminal::ClearType::All);
    let stream = TcpStream::connect("127.0.0.1:5000").await?;
    // This is still writable to as well
    let stream_as_buf = BufReader::new(stream);
    println!("Connected to server");
    let players: AtomicPlayers = Arc::new(RwLock::new(Vec::new()));
    let maze = Arc::new(RwLock::new(Maze::new(1, 1)));
    let server_handle = {
        let players_arc = players.clone();
        let maze_arc = maze.clone();
        tokio::spawn(async move { process(stream_as_buf, players_arc, maze_arc).await })
    };
    let gui_handle = {
        let players_arc = players.clone();
        let maze_arc = maze.clone();
        tokio::spawn(async move { gui(stdout, players_arc, maze_arc).await })
    };
    match tokio::try_join!(server_handle, gui_handle) {
        Ok(_) => (),
        Err(e) => eprint!("Error: {:#?}", e),
    };
    Ok(())
}
