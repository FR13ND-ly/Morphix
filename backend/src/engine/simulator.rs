use rand::{RngExt, SeedableRng, rngs::StdRng};

use crate::models::{
    rule::Action,
    simulation::{SimulationRequest, SimulationResult},
};

use super::rule_evaluator::Context;

pub fn run(
    request: SimulationRequest,
) -> Result<SimulationResult, &'static str> {
    let SimulationRequest {
        automaton,
        grid,
        generations,
        seed,
    } = request;

    let mut current = grid;
    let mut rng = StdRng::seed_from_u64(seed);
    let mut budget = 50_000_000;

    for _ in 0..generations {
        let mut next = current.clone();

        for (row, cells) in current.cells.iter().enumerate() {
            for (col, &cell) in cells.iter().enumerate() {
                let context = Context {
                    grid: &current,
                    automaton: &automaton,
                    row: row as i64,
                    col: col as i64,
                };

                for rule in &automaton.rules {
                    if rule.cell_type != cell.cell_type
                        || context.evaluate(&rule.when, &mut budget)? == 0
                    {
                        continue;
                    }

                    let draw = rng.random::<f64>();
                    let mut cumulative = 0.0;

                    for outcome in &rule.outcomes {
                        cumulative += outcome.probability;

                        if draw < cumulative {
                            let mut updated = cell;

                            for action in &outcome.actions {
                                match action {
                                    Action::SetType { cell_type } => {
                                        updated.cell_type = *cell_type;
                                    }

                                    Action::SetValue { expression } => {
                                        updated.value = context.evaluate(
                                            expression,
                                            &mut budget,
                                        )?;
                                    }
                                }
                            }

                            next.cells[row][col] = updated;
                            break;
                        }
                    }

                    break;
                }
            }
        }

        current = next;
    }

    Ok(SimulationResult {
        grid: current,
        generations,
    })
}