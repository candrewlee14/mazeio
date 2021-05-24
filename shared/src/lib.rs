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
    name: String,
    x: usize,
    y: usize,
}
#[allow(dead_code)]
impl Player {
    pub fn new(name: String) -> Self {
        Self { name, x: 0, y: 0 }
    }
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).map_err(anyhow::Error::msg)
    }
}

#[derive(PartialEq, Eq)]
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

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum CellType {
    Open,
    Wall,
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
    cells: Vec<Vec<CellType>>,
    width: usize,
    height: usize,
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
            println!("x: {}, y: {}", pos_x, pos_y);
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
}
impl std::string::ToString for Maze {
    fn to_string(&self) -> String {
        self.cells
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| match cell {
                        CellType::Wall => '\u{2588}',
                        CellType::Open => ' ',
                    })
                    .chain(std::iter::once('\n'))
                    .collect::<Vec<char>>()
            })
            .flatten()
            .collect::<String>()
    }
}
