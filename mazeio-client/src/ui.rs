use mazeio_shared::{CellType, ProtoMaze};
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
pub use std::rc::Rc;

#[derive(Default)]
pub struct GameView {
    /// A block to wrap the widget in
    state: Option<Rc<RefCell<GameStateSynced>>>,
}
impl GameView {
    pub fn state(mut self, state: Option<Rc<RefCell<GameStateSynced>>>) -> Self {
        self.state = state;
        self
    }
}

impl Widget for GameView {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // if state is not null
        if let Some(state_ref) = self.state {
            // draw maze
            let state = state_ref.borrow();
            let player_pos = state.player_dict[&state.player_id].pos.clone().unwrap();
            let scroll = (player_pos.x, player_pos.y);
            let centering = (area.width / 2, area.height / 2);
            let maze_rows = state
                .maze
                .cells
                .chunks(state.maze.width as usize)
                .skip(scroll.1 as usize)
                .take(area.height.into())
                .map(|row| {
                    row.iter()
                        .skip(scroll.0 as usize)
                        .take(area.width.into())
                        .map(|&i| CellType::from_i32(i).unwrap_or(CellType::Wall))
                        .map(|i| i.to_char())
                        //.chain(std::iter::once('\n'))
                        .collect::<String>()
                })
                .collect::<Vec<String>>();

            // for (i, maze_row) in maze_rows.iter().enumerate() {
            //     let x = (area.x + centering.0) as i32 - scroll.0 as i32;
            //     let y = (area.y + centering.1 + i as u16) as i32 - scroll.1 as i32;
            //     if x >= 0 && x < area.width as i32 && y >= 0 && y < area.height as i32 {
            //         let cell = buf.get_mut(x.try_into().unwrap(), y.try_into().unwrap());
            //         cell.set_char(
            //             CellType::from_i32(state.maze.cells[(y * area.height as i32 + x) as usize])
            //                 .unwrap_or(CellType::Open)
            //                 .to_char(),
            //         )
            //         .set_style(Style::default().fg(Color::Red));
            //         //buf.set_string(area.x + centering.0, y, maze_row, Style::default());
            //     }
            // }

            for i in area.y..area.y + area.height {
                for j in area.x..area.x + area.width {
                    let x =  j as i32 + scroll.0 as i32 - area.x as i32 - centering.0 as i32;
                    let y = i as i32 + scroll.1 as i32 - area.y as i32 - centering.1 as i32;
                    if x >= 0 && x < state.maze.width as i32 && y >= 0 && y < state.maze.height as i32 {
                        let cell = buf.get_mut(j.try_into().unwrap(), i.try_into().unwrap());
                        cell.set_char(
                            CellType::from_i32(
                                state.maze.cells[(y * state.maze.width as i32 + x) as usize],
                            )
                            .unwrap_or(CellType::Open)
                            .to_char(),
                        )
                        .set_style(Style::default());
                        //buf.set_string(area.x + centering.0, y, maze_row, Style::default());
                    }
                }
            }

            // draw opponents
            for (_id, player) in state.player_dict.iter() {
                let pos = (*player).pos.clone().unwrap();
                let x = pos.x as i32 - scroll.0 as i32 + area.x as i32 + centering.0 as i32;
                let y = pos.y as i32 - scroll.1 as i32 + area.y as i32 + centering.1 as i32;
                if x >= 0 && x < area.width as i32 && y >= 0 && y < area.width as i32 {
                    let cell = buf.get_mut(x.try_into().unwrap(), y.try_into().unwrap());
                    cell.set_char('●')
                        .set_style(Style::default().fg(Color::Red));
                }
            }
            let x = player_pos.x as i32 - scroll.0 as i32 + area.x as i32 + centering.0 as i32;
            let y = player_pos.y as i32 - scroll.1 as i32 + area.y as i32 + centering.1 as i32;
            if x >= 0 && x < area.width as i32 && y >= 0 && y < area.width as i32 {
                let cell = buf.get_mut(x.try_into().unwrap(), y.try_into().unwrap());
                // draw client player
                cell.set_char('⬤')
                    .set_style(Style::default().fg(Color::Blue));
            }
        }
    }
}

pub fn ui<B: Backend>(state: Option<Rc<RefCell<GameStateSynced>>>, f: &mut Frame<B>) {
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
    // let size = f.size();
    // let block = Block::default().title("Mazeio").borders(Borders::ALL);
    // f.render_widget(block, size);

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

    let block = Block::default().borders(Borders::LEFT | Borders::RIGHT);
    let game_view = GameView::default().state(state);
    f.render_widget(game_view, chunks[1]);

    let block = Block::default().title("Block 2").borders(Borders::ALL);
    f.render_widget(block, chunks[2]);
}
