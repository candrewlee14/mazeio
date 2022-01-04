extern crate mazeio_shared;
use mazeio_shared::*;

mod ui;
use ui::*;

mod model;
use model::*;

use futures_util::{StreamExt, TryStreamExt};
use mazeio_proto::game_client::GameClient;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc::Sender, RwLock};
use tokio_stream::wrappers::ReceiverStream;
use tonic::Request;

// tui uses
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyCode, KeyEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{error::Error, io};

async fn handle_event(
    is_running: &mut bool,
    event: crossterm::event::Event,
    tx: &Sender<InputDirection>,
) -> Result<(), Box<dyn std::error::Error>> {
    match event {
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
                    *is_running = false;
                }
                _ => {}
            };
        }
        Event::Resize(..) => {}
        _ => {}
    }
    Ok(())
}

async fn run_app<B: Backend>(
    mut game_state: GameState,
    terminal: &mut Terminal<B>,
    tx: &Sender<InputDirection>,
) -> Result<(), Box<dyn std::error::Error>> {
    //let mut events = EventStream::new();
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
    let mut is_running = true;
    let game_state_synced = Rc::new(RefCell::new(game_state.to_synced().await));

    while is_running {
        terminal.draw(|f| ui(Some(game_state_synced.clone()), f))?;
        match crossterm::event::poll(std::time::Duration::from_millis(5))? {
            true => handle_event(&mut is_running, crossterm::event::read()?, tx).await?,
            false => {}
        };
        // clear keyboard buffer
        while crossterm::event::poll(std::time::Duration::from_millis(5))? {
            crossterm::event::read()?;
        }
        let has_changed = game_state.changed_since_synced.lock().await;
        if *has_changed == true {
            drop(has_changed);
            if let Ok(mut state_synced_mut) = game_state_synced.try_borrow_mut() {
                (*state_synced_mut).update_players(&mut game_state).await;
            }
        }
        interval.tick().await;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = GameClient::connect("http://[::1]:50051").await?;
    let game_state = GameState::initial_state("test-name".to_string(), &mut client).await?;

    // buffer to hold the direction values to be sent
    let (tx, rx) = tokio::sync::mpsc::channel(3);
    game_state.handle_player_stream(rx, &mut client).await?;

    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // run app with UI
    run_app(game_state, &mut terminal, &tx).await?;

    // restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen,)?;
    terminal.show_cursor()?;

    Ok(())
}
