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
type AtomicIdOption = Arc<RwLock<Option<u64>>>;

async fn print_char_grid(stdout: AtomicStdout, grid: &BufferGrid) -> Result<()> {
    let mut stdout_accessor = stdout.lock().await;
    for y in 0..grid.len() {
        for (x, (back, fore, ch)) in grid[y].iter().enumerate() {
            stdout_accessor
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

async fn queue_print_players(
    grid: &mut BufferGrid,
    players: &[Player],
    id: AtomicIdOption,
) -> Result<()> {
    let id_val: Option<u64> = {
        let id_readable = id.read().await;
        *id_readable
    };
    let mut myself: Option<&Player> = None;
    for player in players.iter() {
        if Some(player.id) == id_val {
            myself = Some(player);
        } else {
            grid[player.y][player.x] = (Color::Black, Color::DarkBlue, '\u{2B24}');
        }
    }
    if let Some(me) = myself {
        grid[me.y][me.x] = (Color::Black, Color::Cyan, '\u{2B24}');
    }
    Ok(())
}
async fn send_input(
    end_game: AtomicBool,
    mut write_stream: OwnedWriteHalf,
    cur_dir_rwlock: AtomicDirectionVec,
) -> Result<()> {
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(25));
    loop {
        interval.tick().await;
        {
            let end = end_game.read().await;
            if *end {
                return Ok(());
            }
        }
        let mut cur_dir = cur_dir_rwlock.write().await;
        if cur_dir.len() > 0 {
            let mut dir_ser = serde_json::to_string(&cur_dir.pop().unwrap())?;
            dir_ser.push('\n');
            match tokio::time::timeout(
                std::time::Duration::from_millis(15),
                write_stream.write_all(dir_ser.as_bytes()),
            )
            .await
            {
                Ok(Ok(_)) => (),
                Ok(Err(_)) | Err(_) => {
                    println!("Yeet");
                    let mut end = end_game.write().await;
                    *end = true;
                    return Err("Send Failed").map_err(anyhow::Error::msg);
                }
            };
        }
    }
}

async fn process(
    end_game: AtomicBool,
    mut stream_as_buf: BufReader<OwnedReadHalf>,
    players: AtomicPlayers,
    maze: AtomicMazeOption,
    my_id: AtomicIdOption,
) -> Result<()> {
    let mut input = String::new();
    stream_as_buf.read_line(&mut input).await?;
    if let Ok(deser_maze) = serde_json::from_str(&input.trim()) {
        let mut maze_writeable = maze.write().await;
        *maze_writeable = Some(deser_maze);
    }
    input.clear();
    stream_as_buf.read_line(&mut input).await?;
    if let Ok(player) = serde_json::from_str::<Player>(&input.trim()) {
        let mut id_writeable = my_id.write().await;
        *id_writeable = Some(player.id);
        println!("Got player id: {}", player.id);
    }
    input.clear();
    loop {
        {
            let end = end_game.read().await;
            if *end {
                return Ok(());
            }
        }
        match tokio::time::timeout(
            std::time::Duration::from_millis(1000),
            stream_as_buf.read_line(&mut input),
        )
        .await
        {
            Ok(Ok(0)) => {}
            Ok(Ok(_bytes)) => {
                if let Ok(deser_players) = serde_json::from_str(&input.trim()) {
                    let mut players_writeable = players.write().await;
                    *players_writeable = deser_players;
                }
                input.clear();
            }
            Ok(Err(_)) | Err(_) => {
                println!("Read Error");
                let mut end = end_game.write().await;
                *end = true;
                return Err("Disconnected from server").map_err(anyhow::Error::msg);
            }
        };
    }
}
async fn gui(
    end_game: AtomicBool,
    stdout: AtomicStdout,
    players: AtomicPlayers,
    maze: AtomicMazeOption,
    cur_dir_rwlock: AtomicDirectionVec,
    my_id: AtomicIdOption,
) -> Result<()> {
    let (size_x, size_y) = crossterm::terminal::size()?;
    let mut buffer_grid: BufferGrid =
        vec![vec![(Color::Black, Color::White, ' '); size_x as usize]; size_y as usize];
    loop {
        {
            let end = end_game.read().await;
            if *end {
                return Ok(());
            }
        }
        if crossterm::event::poll(std::time::Duration::from_millis(15))? {
            match crossterm::event::read()? {
                Event::Key(keyevent) => match keyevent.code {
                    KeyCode::Esc => {
                        let mut stdout_accessor = stdout.lock().await;
                        stdout_accessor
                            .execute(MoveTo(30, 0))?
                            .execute(SetForegroundColor(Color::Green))?
                            .execute(SetBackgroundColor(Color::Black))?
                            .execute(Print("Exiting program"))?;
                        break;
                    }
                    KeyCode::Down => {
                        let mut cur_dir = cur_dir_rwlock.write().await;
                        cur_dir.push(Direction::Down);
                    }
                    KeyCode::Up => {
                        let mut cur_dir = cur_dir_rwlock.write().await;
                        cur_dir.push(Direction::Up);
                    }
                    KeyCode::Left => {
                        let mut cur_dir = cur_dir_rwlock.write().await;
                        cur_dir.push(Direction::Left);
                    }
                    KeyCode::Right => {
                        let mut cur_dir = cur_dir_rwlock.write().await;
                        cur_dir.push(Direction::Right);
                    }
                    _ => (),
                },
                _ => (),
            }
        }
        let maze_readable = maze.read().await;
        if let Some(maze_info) = &*maze_readable {
            queue_print_maze(&mut buffer_grid, &maze_info).await?;
            {
                let players_readable = players.read().await;
                queue_print_players(&mut buffer_grid, &*players_readable, my_id.clone()).await?;
            }
            print_char_grid(stdout.clone(), &buffer_grid).await?;
        }
        let mut stdout_accessor = stdout.lock().await;
        stdout_accessor.flush()?;
    }
    let mut stdout_accessor = stdout.lock().await;
    terminal::disable_raw_mode()?;
    stdout_accessor.execute(terminal::LeaveAlternateScreen)?;
    let mut end = end_game.write().await;
    *end = true;
    Err("Exited function early with escape key").map_err(anyhow::Error::msg)
}

#[tokio::main]
async fn main() -> Result<()> {
    let server_ip = std::env::args()
        .nth(1)
        .unwrap_or("127.0.0.1:5000".to_string());
    let stdout = Arc::new(Mutex::new(stdout()));
    let end_game: AtomicBool = Arc::new(RwLock::new(false));
    {
        let mut stdout_accessor = stdout.lock().await;
        stdout_accessor
            .execute(terminal::EnterAlternateScreen)?
            .execute(cursor::Hide)?;
    }
    terminal::enable_raw_mode()?;
    terminal::Clear(terminal::ClearType::All);

    let my_id: AtomicIdOption = Arc::new(RwLock::new(None));
    let current_dir: AtomicDirectionVec = Arc::new(RwLock::new(Vec::new()));
    let stream = TcpStream::connect(server_ip).await?;
    let (read_stream, write_stream) = stream.into_split();
    let stream_as_buf = BufReader::new(read_stream);

    let players: AtomicPlayers = Arc::new(RwLock::new(Vec::new()));
    let maze = Arc::new(RwLock::new(None));
    let server_handle = {
        let end_game_arc = end_game.clone();
        let players_arc = players.clone();
        let maze_arc = maze.clone();
        let my_id_arc = my_id.clone();
        tokio::spawn(async move {
            process(
                end_game_arc,
                stream_as_buf,
                players_arc,
                maze_arc,
                my_id_arc,
            )
            .await
        })
    };
    let gui_handle = {
        let end_game_arc = end_game.clone();
        let players_arc = players.clone();
        let maze_arc = maze.clone();
        let current_dir_clone = current_dir.clone();
        let stdout_arc = stdout.clone();
        let my_id_arc = my_id.clone();
        tokio::spawn(async move {
            gui(
                end_game_arc,
                stdout_arc,
                players_arc,
                maze_arc,
                current_dir_clone,
                my_id_arc,
            )
            .await
        })
    };
    let send_handle = {
        let end_game_arc = end_game.clone();
        let current_dir_clone = current_dir.clone();
        tokio::spawn(async move { send_input(end_game_arc, write_stream, current_dir_clone).await })
    };
    match tokio::try_join!(server_handle, gui_handle, send_handle) {
        Ok(_) => (),
        Err(e) => println!("Error: {:#?}", e),
    };
    let mut stdout_accessor = stdout.lock().await;
    terminal::disable_raw_mode()?;
    stdout_accessor.execute(terminal::LeaveAlternateScreen)?;
    Ok(())
}
