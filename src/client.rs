mod shared;
use shared::*;

mod ui;
use ui::*;

use futures_util::{StreamExt, TryStreamExt};
use mazeio_proto::game_client::GameClient;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc::Sender, RwLock};
use tokio_stream::wrappers::ReceiverStream;
use tonic::Request;

pub type AtomicPlayerDict = Arc<RwLock<HashMap<String, Player>>>;

pub struct GameState {
    player_id: String,
    maze: ProtoMaze,
    player_dict: AtomicPlayerDict,
}

// tui uses
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyCode, KeyEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{error::Error, io};


impl GameState {
    pub async fn initial_state(
        client: &mut GameClient<tonic::transport::Channel>,
    ) -> Result<Self, tonic::Status> {
        let join_game_response = client
            .connect_player(Request::new(JoinGameRequest {
                name: "test-name".to_string(),
            }))
            .await?
            .into_inner();

        match join_game_response {
            JoinGameResponse {
                maze: Some(maze_val),
                players,
                player_id,
            } => Ok(GameState {
                player_id: player_id,
                maze: maze_val,
                player_dict: Arc::new(RwLock::new(
                    players
                        .iter()
                        .map(|player| (player.id.clone(), player.clone()))
                        .collect::<HashMap<String, Player>>(),
                )),
            }),
            _ => panic!(),
        }
    }
}

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    tx: &Sender<InputDirection>,
) -> io::Result<()> {
    //let mut events = EventStream::new();
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(250));
    loop {
        terminal.draw(|f| ui(f))?;
        match crossterm::event::poll(std::time::Duration::from_millis(50))? {
            true => match crossterm::event::read()? {
                Event::Key(key) => {
                    match key.code {
                        KeyCode::Char('a') => {
                            tx.send(InputDirection {
                                direction: Direction::Left.into(),
                            })
                            .await
                            .unwrap();
                        }
                        KeyCode::Char('d') => {
                            tx.send(InputDirection {
                                direction: Direction::Right.into(),
                            })
                            .await
                            .unwrap();
                        }
                        KeyCode::Char('w') => {
                            tx.send(InputDirection {
                                direction: Direction::Up.into(),
                            })
                            .await
                            .unwrap();
                        }
                        KeyCode::Char('s') => {
                            tx.send(InputDirection {
                                direction: Direction::Down.into(),
                            })
                            .await
                            .unwrap();
                        }
                        KeyCode::Esc => {
                            break;
                        }
                        _ => {}
                    };
                }
                Event::Resize(..) => {}
                _ => {}
            },
            false => {}
        }
        // clear keyboard buffer
        while crossterm::event::poll(std::time::Duration::from_millis(25))? {
            crossterm::event::read()?;
        }
        interval.tick().await;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = GameClient::connect("http://[::1]:50051").await?;
    let game_state = GameState::initial_state(&mut client).await?;
    //println!("My Player ID: {}", game_state.player_id);
    //println!("Maze:\n{}", game_state.maze.to_string());

    // buffer to hold the direction values to be sent
    let (tx, rx) = tokio::sync::mpsc::channel(3);

    tokio::spawn(async move {
        let mut playerStream = client
            .stream_game(ReceiverStream::new(rx))
            .await
            .unwrap()
            .into_inner();

        while let Some(res) = playerStream.next().await {
            if let Ok(player) = res {
                let mut player_dict_lock = game_state.player_dict.write().await;
                if !player.alive {
                    (*player_dict_lock).remove(&player.id);
                } else {
                    (*player_dict_lock).insert(player.id.clone(), player);
                    //println!("{:#?}\n", (*player_dict_lock));
                }
            } else {
                println! {"{:?}", res};
                break;
            }
        }
    });

    // send random directions
    for _i in 0..3 {
        tx.send(InputDirection {
            direction: rand::random::<Direction>().into(),
        })
        .await
        .unwrap();
    }

    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // run app with UI
    run_app(&mut terminal, &tx).await?;

    // restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen,)?;
    terminal.show_cursor()?;

    Ok(())
}
