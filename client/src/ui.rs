use mazeio_shared::{CellType, Position, ProtoMaze};
use tui::{buffer::Buffer, layout::Rect};

pub use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget, Wrap},
    Frame, Terminal,
};
pub use unicode_width::UnicodeWidthStr;

use super::model::GameStateSynced;
pub use core::cell::RefCell;
use std::collections::HashSet;
pub use std::rc::Rc;

pub struct GameView {
    state: Option<Rc<RefCell<GameStateSynced>>>,
    pos_history: Rc<RefCell<HashSet<(u32, u32)>>>,
}

fn draw_player(
    pos_history: Rc<RefCell<HashSet<(u32, u32)>>>,
    pos: Position,
    ch: char,
    fg_col: Color,
    scroll: &(u32, u32),
    centering: &(u16, u16),
    area: &Rect,
    buf: &mut Buffer,
) {
    let x = pos.x as i32 - scroll.0 as i32 + area.x as i32 + centering.0 as i32;
    let y = pos.y as i32 - scroll.1 as i32 + area.y as i32 + centering.1 as i32;
    if x >= area.x as i32
        && x < (area.x + area.width) as i32
        && y >= area.y as i32
        && y < (area.y + area.height) as i32
    {
        let cell = buf.get_mut(x.try_into().unwrap(), y.try_into().unwrap());
        let mut style = Style::default().fg(fg_col);
        if let Ok(pos_history_inner) = pos_history.try_borrow_mut() {
            if (*pos_history_inner).contains(&(pos.x, pos.y)) {
                style = style.bg(Color::Blue);
            }
        }
        cell.set_char(ch).set_style(style);
    }
}

impl Widget for GameView {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // if state is not null
        match self {
            GameView {
                state: Some(state_ref),
                pos_history,
            } => {
                let state = state_ref.borrow();
                let player_pos = state.player_dict[&state.player_id].pos.clone().unwrap();
                let scroll = (player_pos.x, player_pos.y);
                let centering = (area.width / 2, area.height / 2);

                for i in area.y..area.y + area.height {
                    for j in area.x..area.x + area.width {
                        let x = j as i32 + scroll.0 as i32 - area.x as i32 - centering.0 as i32;
                        let y = i as i32 + scroll.1 as i32 - area.y as i32 - centering.1 as i32;
                        if x >= 0
                            && x < state.maze.width as i32
                            && y >= 0
                            && y < state.maze.height as i32
                        {
                            let cell = buf.get_mut(j.try_into().unwrap(), i.try_into().unwrap());
                            let mut style = Style::default();
                            if let Ok(pos_history_inner) = pos_history.try_borrow_mut() {
                                if (*pos_history_inner).contains(&(x as u32, y as u32)) {
                                    style = style.bg(Color::Blue);
                                }
                            }
                            cell.set_char(
                                CellType::from_i32(
                                    state.maze.cells[(y * state.maze.width as i32 + x) as usize],
                                )
                                .unwrap_or(CellType::Open)
                                .to_char(),
                            )
                            .set_style(style);
                        }
                    }
                }

                // draw opponents
                for (_id, player) in state.player_dict.clone().iter() {
                    let pos = (*player).pos.clone().unwrap();
                    draw_player(
                        pos_history.clone(),
                        pos,
                        '●',
                        Color::Red,
                        &scroll.clone(),
                        &centering,
                        &area,
                        buf,
                    );
                }
                draw_player(
                    pos_history,
                    player_pos,
                    '●',
                    Color::Cyan,
                    &scroll,
                    &centering,
                    &area,
                    buf,
                );
            }
            _ => {}
        }
    }
}

pub fn ui<B: Backend>(
    state: Option<Rc<RefCell<GameStateSynced>>>,
    pos_history: Rc<RefCell<HashSet<(u32, u32)>>>,
    f: &mut Frame<B>,
) {
    let chunks = Layout::default()
        .direction(tui::layout::Direction::Vertical)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Percentage(80),
                Constraint::Percentage(10),
            ]
            .as_ref(),
        )
        .split(f.size());

    // blocks
    let block = Block::default().borders(Borders::ALL).title(Span::styled(
        "Mazeio",
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
    ));
    let text = vec![Spans::from(vec![
        Span::raw("Welcome to "),
        Span::styled(
            "mazeio",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(". the online multiplayer maze game!"),
    ])];
    let paragraph = Paragraph::new(text).block(block).wrap(Wrap { trim: true });
    f.render_widget(paragraph, chunks[0]);

    // game section
    let block = Block::default().borders(Borders::LEFT | Borders::RIGHT);
    let game_view = GameView {
        state,
        pos_history: pos_history.clone(),
    };
    f.render_widget(game_view, block.inner(chunks[1]));
    f.render_widget(block, chunks[1]);

    let block = Block::default().title("Instructions").borders(Borders::ALL);
    let text = vec![Spans::from(vec![
        Span::raw("Use "),
        Span::styled(
            "WASD",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" to control your player. Press "),
        Span::styled(
            "Esc",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" to exit."),
    ])];
    let paragraph = Paragraph::new(text).block(block).wrap(Wrap { trim: true });
    f.render_widget(paragraph, chunks[2]);
}
