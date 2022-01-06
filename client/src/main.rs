extern crate mazeio_shared;
use mazeio_shared::*;

mod ui;
use ui::*;

mod model;
use model::*;

use mazeio_proto::game_client::GameClient;
use std::collections::HashSet;
use tokio::sync::mpsc::Sender;
// tui uses
use crossterm::{
    event::{Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io;

async fn handle_event(
    is_running: &mut bool,
    event: crossterm::event::Event,
    tx: &Sender<InputDirection>,
    pos_history: Rc<RefCell<HashSet<(u32, u32)>>>,
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
        // locally move the player (this will be overwritten at later syncing)
        if let Some(player) = game_state_synced
            .player_dict
            .get_mut(&game_state_synced.player_id)
        {
            (*player).move_if_valid(&game_state_synced.maze, dir);
            if let Some(pos) = (*player).pos.clone() {
                if let Ok(mut pos_history_mut) = pos_history.try_borrow_mut() {
                    (*pos_history_mut).insert((pos.x, pos.y));
                }
            }

            // spawn a short thread to send the input to server
            let tx_clone = tx.clone();
            tokio::spawn(async move {
                tx_clone
                    .send(InputDirection {
                        direction: dir.into(),
                    })
                    .await
                    .unwrap();
            });
        }
    }
    Ok(())
}

async fn run_app<B: Backend>(
    mut game_state: GameState,
    terminal: &mut Terminal<B>,
    tx: &Sender<InputDirection>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
    let mut is_running = true;
    let game_state_synced = Rc::new(RefCell::new(game_state.to_synced().await));
    let pos_history = Rc::new(RefCell::new(HashSet::with_capacity(100)));
    // let mut frame_num: u128 = 0;
    // let mut total_time: u128 = 0;
    while is_running {
        // frame_num += 1;
        //let now = Instant::now();
        if let Ok(true) = crossterm::event::poll(std::time::Duration::from_millis(5)) {
            // println!("Got input!");
            if let Ok(mut state_synced_mut) = game_state_synced.try_borrow_mut() {
                handle_event(
                    &mut is_running,
                    crossterm::event::read()?,
                    tx,
                    pos_history.clone(),
                    &mut state_synced_mut,
                )
                .await?;
            }
        };
        //clear keyboard buffer
        while crossterm::event::poll(std::time::Duration::from_millis(1))? {
            crossterm::event::read()?;
        }
        //draw
        terminal.draw(|f| ui(Some(game_state_synced.clone()), pos_history.clone(), f))?;

        let has_changed = game_state.changed_since_synced.lock().await;
        if *has_changed == true {
            drop(has_changed);
            if let Ok(mut state_synced_mut) = game_state_synced.try_borrow_mut() {
                (*state_synced_mut).update_players(&mut game_state).await;
            }
        }

        //total_time += now.elapsed().as_millis();
        //println!("Avg time: {:?}", total_time / frame_num);
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
