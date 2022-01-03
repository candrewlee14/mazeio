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
    scroll: (u16, u16),
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
        let x = area.left();
        let y = area.top();
        // if state is not null
        if let Some(state_ref) = self.state {
            // draw maze
            let state = state_ref.borrow();
            let maze_str = state
                .maze
                .cells
                .chunks(state.maze.width as usize)
                .skip(self.scroll.1.into())
                .take(area.height.into())
                .map(|row| {
                    row.iter()
                        .skip(self.scroll.0.into())
                        .take(area.width.into())
                        .map(|&i| CellType::from_i32(i).unwrap_or(CellType::Wall))
                        .map(|i| i.to_char())
                        .chain(std::iter::once('\n'))
                        .collect::<Vec<char>>()
                })
                .flatten()
                .collect::<String>();
            buf.set_string(x, y, maze_str, Style::default());

            // draw opponents
            for (_id, player) in state.player_dict.iter() {
                let pos = (*player).pos.clone().unwrap();
                let mut cell = buf.get_mut(
                    (pos.x + self.scroll.0 as u32).try_into().unwrap(),
                    (pos.y + self.scroll.1 as u32).try_into().unwrap(),
                );
                cell.set_char('•')
                    .set_style(Style::default().fg(Color::Red));
            }
            let pos = state.player_dict[&state.player_id].pos.clone().unwrap();
            let mut cell = buf.get_mut(
                (pos.x + self.scroll.0 as u32).try_into().unwrap(),
                (pos.y + self.scroll.1 as u32).try_into().unwrap(),
            );
            // draw client player
            cell.set_char('•')
                .set_style(Style::default().fg(Color::Red));
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

    let block = Block::default().title("Block 2").borders(Borders::ALL);
    f.render_widget(block, chunks[2]);
}
