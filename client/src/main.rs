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
use tokio::time::Instant;
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
    game_state_synced: &mut GameStateSynced,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut maybe_dir: Option<Direction> = None;
    match event {
        Event::Key(key) => {
            match key.code {
                KeyCode::Char('a') => {
                    maybe_dir = Some(Direction::Left);
                }
                KeyCode::Char('d') => {
                    maybe_dir = Some(Direction::Right);
                }
                KeyCode::Char('w') => {
                    maybe_dir = Some(Direction::Up);
                }
                KeyCode::Char('s') => {
                    maybe_dir = Some(Direction::Down);
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
    if let Some(dir) = maybe_dir {
        if let Some(player) = game_state_synced
            .player_dict
            .get_mut(&game_state_synced.player_id)
        {
            (*player).move_if_valid(&game_state_synced.maze, dir);
        }
        tx.send(InputDirection {
            direction: dir.into(),
        })
        .await
        .unwrap();
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
        match crossterm::event::poll(std::time::Duration::from_millis(5))? {
            true => {
                // println!("Got input!");
                if let Ok(mut state_synced_mut) = game_state_synced.try_borrow_mut() {
                    handle_event(
                        &mut is_running,
                        crossterm::event::read()?,
                        tx,
                        &mut state_synced_mut,
                    )
                    .await?;
                }
            }
            false => {}
        };
        terminal.draw(|f| ui(Some(game_state_synced.clone()), f))?;

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
    let (tx, rx) = tokio::sync::mpsc::channel(5);
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