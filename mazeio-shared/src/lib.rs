pub use tonic;
pub use tokio;
pub use tokio_stream;
pub use futures_core;
pub use futures_util;
pub use rand;
pub use uuid;

pub mod mazeio_proto {
    tonic::include_proto!("mazeio");
}

pub use mazeio_proto::{
    CellType, Direction, InputDirection, JoinGameRequest, JoinGameResponse, Maze as ProtoMaze,
    Player, Position,
};

use rand::{
    distributions::{Distribution, Standard},
    Rng,
};
use std::cmp::{max, min};
use uuid::Uuid;

#[allow(unused)]
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
#[allow(unused)]
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
#[allow(unused)]
impl Position {
    pub fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }
    pub fn move_in_dir(
        &mut self,
        min_x: usize,
        min_y: usize,
        max_x: usize,
        max_y: usize,
        dir: Direction,
        dist: usize,
    ) {
        let mut x = self.x as usize;
        let mut y = self.y as usize;
        move_in_dir(&mut x, &mut y, min_x, min_y, max_x, max_y, &dir, dist);
        self.x = x as u32;
        self.y = y as u32;
    }
}
#[allow(unused)]
impl Player {
    pub fn new(name: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            pos: Some(Position::new(1, 1)),
            alive: true,
        }
    }
    pub fn move_if_valid(&mut self, maze: &ProtoMaze, dir: Direction) -> bool {
        let mut pos = self.pos.clone().unwrap();
        pos.move_in_dir(0, 0, maze.width as usize, maze.height as usize, dir, 1);
        if maze.get(pos.x as usize, pos.y as usize) == CellType::Open {
            self.pos = Some(pos);
            return true;
        }
        false
    }
}

#[allow(unused)]
impl ProtoMaze {
    pub fn get(&self, x: usize, y: usize) -> CellType {
        CellType::from_i32(self.cells[y * self.width as usize + x]).unwrap_or(CellType::Wall)
    }
    pub fn set(&mut self, x: usize, y: usize, val: CellType) {
        self.cells[y * self.width as usize + x] = val as i32;
    }
    pub fn new(open_cells_x: usize, open_cells_y: usize) -> Self {
        let mut width = open_cells_x * 2;
        width += (width + 1) % 2;
        let mut height = open_cells_y * 2;
        height += (height + 1) % 2;
        let mut pos_x = 1;
        let mut pos_y = 1;
        let mut maze = ProtoMaze {
            width: width as u32,
            height: height as u32,
            cells: vec![CellType::Wall as i32; (width * height) as usize],
        };
        maze.set(pos_x, pos_y, CellType::Open);
        let mut total_open_cells = open_cells_x * open_cells_y - 1;
        while total_open_cells > 0 {
            let dir: Direction = rand::random();
            move_in_dir(&mut pos_x, &mut pos_y, 1, 1, width - 2, height - 2, &dir, 2);
            if maze.get(pos_x, pos_y) == CellType::Wall {
                maze.set(pos_x, pos_y, CellType::Open);
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
                maze.set(between_x, between_y, CellType::Open);
                total_open_cells -= 1;
            }
        }
        maze
    }
}
impl std::string::ToString for ProtoMaze {
    fn to_string(&self) -> String {
        let maze_str = self
            .cells
            .chunks(self.width as usize)
            .map(|row| {
                row.iter()
                    .map(|&i| CellType::from_i32(i).unwrap_or(CellType::Wall))
                    .map(|i| i.to_char())
                    .chain(std::iter::once('\n'))
                    .collect::<Vec<char>>()
            })
            .flatten()
            .collect::<String>();
        maze_str
    }
}

impl CellType {
    pub fn to_char(&self) -> char {
        match self {
            CellType::Wall => '\u{2588}',
            CellType::Open => ' ',
        }
    }
}

// TODO make more tests
#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
