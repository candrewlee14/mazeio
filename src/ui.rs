pub use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
pub use unicode_width::UnicodeWidthStr;

pub struct GameWidget<'a> {
    /// A block to wrap the widget in
    block: Option<Block<'a>>,
    scroll: (u16, u16),
}
impl<'a> GameWidget<'a> {
    pub fn new() -> GameWidget<'a>
    {
        GameWidget {
            block: None,
            scroll: (0, 0),
        }
    }
    pub fn block(mut self, block: Block<'a>) -> GameWidget<'a> {
        self.block = Some(block);
        self
    }
    pub fn scroll(mut self, offset: (u16, u16)) -> GameWidget<'a> {
        self.scroll = offset;
        self
    }
}

pub fn ui<B: Backend>(f: &mut Frame<B>) {
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

    let block = Block::default().title("Block 2").borders(Borders::ALL);
    f.render_widget(block, chunks[2]);
}
