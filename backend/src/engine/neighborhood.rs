use crate::models::{
    automaton::Boundary,
    cell::Cell,
    grid::Grid,
};

pub fn read(
    grid: &Grid,
    boundary: Boundary,
    row: i64,
    col: i64,
) -> Cell {
    let height = grid.cells.len() as i64;
    let width = grid.cells[0].len() as i64;

    match boundary {
        Boundary::Fixed { cell } => {
            if row < 0 || col < 0 || row >= height || col >= width {
                cell
            } else {
                grid.cells[row as usize][col as usize]
            }
        }

        Boundary::Wrap => {
            let row = row.rem_euclid(height) as usize;
            let col = col.rem_euclid(width) as usize;

            grid.cells[row][col]
        }
    }
}