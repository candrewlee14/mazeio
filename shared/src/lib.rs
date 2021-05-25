use anyhow::Result;
use rand::{
    distributions::{Distribution, Standard},
    Rng,
};
use serde::{Deserialize, Serialize};
use serde_json;
use std::cell::Cell;
use std::cmp::{max, min};

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Debug)]
pub struct Player {
    pub name: String,
    pub x: usize,
    pub y: usize,
}
#[allow(dead_code)]
impl Player {
    pub fn new(name: String) -> Self {
        Self { name, x: 1, y: 1 }
    }
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).map_err(anyhow::Error::msg)
    }
}

#[repr(u8)]
#[derive(PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}
impl Direction {
    pub fn flip(&self) -> Self {
        match self {
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
        }
    }
}
impl Distribution<Direction> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Direction {
        match rng.gen_range(0..=3) {
            0 => Direction::Left,
            1 => Direction::Right,
            2 => Direction::Up,
            _ => Direction::Down,
        }
    }
}

#[repr(u8)]
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum CellType {
    Open,
    Wall,
}
impl CellType {
    pub fn to_char(&self) -> char {
        match self {
            CellType::Wall => '\u{2588}',
            CellType::Open => ' ',
        }
    }
}

#[inline]
pub fn move_in_dir(
    x: &mut usize,
    y: &mut usize,
    min_x: usize,
    min_y: usize,
    max_x: usize,
    max_y: usize,
    dir: &Direction,
    dist: usize,
) {
    match *dir {
        Direction::Left => {
            *x = max(x.saturating_sub(dist), min_x);
        }
        Direction::Right => {
            *x = min(*x + dist, max_x);
        }
        Direction::Down => {
            *y = min(*y + dist, max_y);
        }
        Direction::Up => {
            *y = max(y.saturating_sub(dist), min_y);
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Maze {
    pub cells: Vec<Vec<CellType>>,
    pub width: usize,
    pub height: usize,
}
impl Maze {
    pub fn new(open_cells_x: usize, open_cells_y: usize) -> Self {
        let mut width = open_cells_x * 2;
        width += (width + 1) % 2;
        let mut height = open_cells_y * 2;
        height += (height + 1) % 2;
        let mut cells = vec![vec![CellType::Wall; width]; height];
        let mut pos_x = 1;
        let mut pos_y = 1;
        cells[pos_y][pos_x] = CellType::Open;
        let mut total_open_cells = open_cells_x * open_cells_y - 1;
        while total_open_cells > 0 {
            let dir: Direction = rand::random();
            move_in_dir(&mut pos_x, &mut pos_y, 1, 1, width - 2, height - 2, &dir, 2);
            if cells[pos_y][pos_x] == CellType::Wall {
                cells[pos_y][pos_x] = CellType::Open;
                let mut between_x = pos_x;
                let mut between_y = pos_y;
                move_in_dir(
                    &mut between_x,
                    &mut between_y,
                    1,
                    1,
                    width - 1,
                    height - 1,
                    &dir.flip(),
                    1,
                );
                cells[between_y][between_x] = CellType::Open;
                total_open_cells -= 1;
            }
        }
        Self {
            cells,
            width,
            height,
        }
    }
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).map_err(anyhow::Error::msg)
    }
}
impl std::string::ToString for Maze {
    fn to_string(&self) -> String {
        self.cells
            .iter()
            .map(|row| {
                row.iter()
                    .map(CellType::to_char)
                    .chain(std::iter::once('\n'))
                    .collect::<Vec<char>>()
            })
            .flatten()
            .collect::<String>()
    }
}
