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
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, RwLock};

type AtomicPlayers = Arc<RwLock<Vec<Player>>>;
type AtomicDirectionOption = Arc<RwLock<Option<Direction>>>;
type AtomicMazeOption = Arc<RwLock<Option<Maze>>>;
type BufferGrid = Vec<Vec<(Color, Color, char)>>;

async fn print_char_grid(stdout: &mut Stdout, grid: &BufferGrid) -> Result<()> {
    for y in 0..grid.len() {
        for (x, (back, fore, ch)) in grid[y].iter().enumerate() {
            stdout
                .queue(MoveTo(x as u16, y as u16))?
                .queue(SetBackgroundColor(*back))?
                .queue(SetForegroundColor(*fore))?
                .queue(Print(ch))?;
        }
    }
    Ok(())
}
async fn queue_print_maze(grid: &mut BufferGrid, maze: &Maze) -> Result<()> {
    for y in 0..maze.cells.len() {
        for (x, cell) in maze.cells[y].iter().enumerate() {
            grid[y][x] = (Color::Black, Color::White, cell.to_char());
        }
    }
    Ok(())
}

async fn queue_print_players(grid: &mut BufferGrid, players: &[Player]) -> Result<()> {
    for player in players.iter() {
        grid[player.y][player.x] = (Color::Cyan, Color::Cyan, '\u{2588}');
    }
    Ok(())
}
async fn send_input(
    mut write_stream: WriteHalf<TcpStream>,
    cur_dir_rwlock: AtomicDirectionOption,
) -> Result<()> {
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(25));
    loop {
        interval.tick().await;
        let cur_dir = cur_dir_rwlock.read().await;
        if let Some(dir) = &*cur_dir {
            let mut dir_ser = serde_json::to_string(dir)?;
            //println!("{}", dir_ser);
            dir_ser.push('\n');
            write_stream.write_all(dir_ser.as_bytes()).await?;
            //println!("Sent data");
        }
    }
}

async fn process(
    mut stream_as_buf: BufReader<ReadHalf<TcpStream>>,
    players: AtomicPlayers,
    maze: AtomicMazeOption,
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
                    *maze_writeable = Some(deser_maze);
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
async fn gui(
    mut stdout: Stdout,
    players: AtomicPlayers,
    maze: AtomicMazeOption,
    cur_dir_rwlock: AtomicDirectionOption,
) -> Result<()> {
    let (size_x, size_y) = crossterm::terminal::size()?;
    let mut buffer_grid: BufferGrid =
        vec![vec![(Color::Black, Color::White, ' '); size_x as usize]; size_y as usize];
    loop {
        if crossterm::event::poll(std::time::Duration::from_millis(25))? {
            match crossterm::event::read()? {
                Event::Key(keyevent) => match keyevent.code {
                    KeyCode::Esc => {
                        stdout
                            .execute(MoveTo(30, 0))?
                            .execute(SetForegroundColor(Color::Green))?
                            .execute(SetBackgroundColor(Color::Black))?
                            .execute(Print("Exiting program"))?;
                        break;
                    }
                    KeyCode::Down => {
                        let mut cur_dir = cur_dir_rwlock.write().await;
                        *cur_dir = Some(Direction::Down);
                    }
                    KeyCode::Up => {
                        let mut cur_dir = cur_dir_rwlock.write().await;
                        *cur_dir = Some(Direction::Up);
                    }
                    KeyCode::Left => {
                        let mut cur_dir = cur_dir_rwlock.write().await;
                        *cur_dir = Some(Direction::Left);
                    }
                    KeyCode::Right => {
                        let mut cur_dir = cur_dir_rwlock.write().await;
                        *cur_dir = Some(Direction::Right);
                    }
                    _ => (),
                },
                _ => (),
            }
        } else {
            {
                let mut cur_dir = cur_dir_rwlock.write().await;
                *cur_dir = None;
            }
            let maze_readable = maze.read().await;
            if let Some(maze_info) = &*maze_readable {
                queue_print_maze(&mut buffer_grid, &maze_info).await?;
                {
                    let players_readable = players.read().await;
                    queue_print_players(&mut buffer_grid, &*players_readable).await?;
                }
                print_char_grid(&mut stdout, &buffer_grid).await?;
            }
        }
        stdout.flush()?;
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
    let current_dir: AtomicDirectionOption = Arc::new(RwLock::new(None));
    let stream = TcpStream::connect("127.0.0.1:5000").await?;
    let (read_stream, write_stream) = tokio::io::split(stream);
    let stream_as_buf = BufReader::new(read_stream);
    //println!("Connected to server");
    let players: AtomicPlayers = Arc::new(RwLock::new(Vec::new()));
    let maze = Arc::new(RwLock::new(None));
    let server_handle = {
        let players_arc = players.clone();
        let maze_arc = maze.clone();
        tokio::spawn(async move { process(stream_as_buf, players_arc, maze_arc).await })
    };
    let gui_handle = {
        let players_arc = players.clone();
        let maze_arc = maze.clone();
        let current_dir_clone = current_dir.clone();
        tokio::spawn(async move { gui(stdout, players_arc, maze_arc, current_dir_clone).await })
    };
    let send_handle = {
        let current_dir_clone = current_dir.clone();
        tokio::spawn(async move { send_input(write_stream, current_dir_clone).await })
    };
    match tokio::try_join!(server_handle, gui_handle, send_handle) {
        Ok(_) => (),
        Err(e) => eprint!("Error: {:#?}", e),
    };
    Ok(())
}
