use std::collections::{BTreeMap, HashMap, HashSet};

use rand::{Rng, RngExt, SeedableRng, rngs::StdRng};

use crate::models::{
    action::{
        ActionResult, AgentSpawn, CellUpdate, Movement, NeighborPropertyUpdate, NeighborSelection,
    },
    agent::Agent,
    automaton::{AutomatonDefinition, Boundary},
    position::Position,
    rule::{RuleTarget, apply_rules_validated},
    simulation::{MAX_HISTORY_UNITS, SimulationGeneration, world_history_units},
    world::World,
};

struct AgentDecision {
    agent: Agent,
    result: ActionResult,
    destination: Position,
}

pub fn step(
    automaton: &AutomatonDefinition,
    current: &World,
    rng: &mut impl Rng,
) -> Result<World, String> {
    automaton.validate_world(current)?;
    let mut last_agent_id = current
        .agents
        .iter()
        .map(|agent| agent.id)
        .max()
        .unwrap_or(0);
    step_with_ids(automaton, current, rng, &mut last_agent_id)
}

fn step_with_ids(
    automaton: &AutomatonDefinition,
    current: &World,
    rng: &mut impl Rng,
    last_agent_id: &mut u64,
) -> Result<World, String> {
    let mut next = current.clone();
    next.agents.clear();
    let mut spawns = Vec::new();
    let mut cell_updates = Vec::new();
    let mut neighbor_updates = Vec::new();

    for (row, cells) in current.cells.iter().enumerate() {
        for (col, state) in cells.iter().enumerate() {
            let neighbors = automaton.neighborhood.read_validated(
                current,
                Position { row, col },
                &automaton.boundary,
            );

            let result =
                apply_rules_validated(RuleTarget::Cell, state, state, &neighbors, automaton, rng)
                    .map_err(|error| format!("Cell ({row}, {col}): {error}"))?;
            next.cells[row][col] = result.state;
            for update in result.neighbor_updates {
                neighbor_updates.push((Position { row, col }, update));
            }
            if let Some(spawn) = result.spawn {
                spawns.push((Position { row, col }, spawn));
            }
        }
    }

    let mut decisions = Vec::with_capacity(current.agents.len());
    let occupied: HashSet<_> = current.agents.iter().map(|agent| agent.position).collect();
    for agent in &current.agents {
        let neighbors =
            automaton
                .neighborhood
                .read_validated(current, agent.position, &automaton.boundary);

        let cell = &current.cells[agent.position.row][agent.position.col];
        let result = apply_rules_validated(
            RuleTarget::Agent,
            &agent.state,
            cell,
            &neighbors,
            automaton,
            rng,
        )
        .map_err(|error| format!("Agent {}: {error}", agent.id))?;
        let destination = match result.movement {
            Some(Movement::Offset(offset)) => {
                offset_destination(agent.position, offset, current, &automaton.boundary)
                    .unwrap_or(agent.position)
            }
            Some(Movement::FreeCell {
                selection,
                cell_type,
            }) => choose_free_cell(current, &occupied, selection, cell_type, rng)
                .unwrap_or(agent.position),
            Some(Movement::RemovedNeighbor) => {
                let mut destination = agent.position;
                for target in &current.agents {
                    if Some(target.id) == result.neighbor_to_remove {
                        destination = target.position;
                        break;
                    }
                }
                destination
            }
            None => agent.position,
        };
        decisions.push(AgentDecision {
            agent: agent.clone(),
            result,
            destination,
        });
    }

    let removed_neighbors = resolve_neighbor_removals(&mut decisions);
    decisions.retain(|decision| {
        !decision.result.remove_agent && !removed_neighbors.contains(&decision.agent.id)
    });

    for decision in &mut decisions {
        let agent = &mut decision.agent;
        let result = &mut decision.result;
        if let Some(update) = result.cell_update.take() {
            cell_updates.push((agent.position, agent.id, update));
        }
        if let Some(spawn) = result.spawn.take() {
            spawns.push((agent.position, spawn));
        }
        agent.state = result.state.clone();
    }

    apply_cell_updates(automaton, &mut next, cell_updates)?;
    apply_neighbor_updates(automaton, &mut next, neighbor_updates)?;

    // Survivors still have their original positions when movement is resolved.
    resolve_moves(&mut decisions);
    for mut decision in decisions {
        decision.agent.position = decision.destination;
        next.agents.push(decision.agent);
    }
    add_spawned_agents(automaton, &mut next, spawns, last_agent_id)?;

    next.validate()?;
    Ok(next)
}

fn choose_free_cell(
    world: &World,
    occupied: &HashSet<Position>,
    selection: NeighborSelection,
    cell_type: Option<u8>,
    rng: &mut impl Rng,
) -> Option<Position> {
    let mut candidates = Vec::new();
    for (row, cells) in world.cells.iter().enumerate() {
        for (col, cell) in cells.iter().enumerate() {
            let position = Position { row, col };
            if occupied.contains(&position) {
                continue;
            }
            if let Some(type_id) = cell_type {
                if cell.type_id != type_id {
                    continue;
                }
            }
            candidates.push(position);
        }
    }
    match selection {
        NeighborSelection::First => candidates.first().copied(),
        NeighborSelection::Random if !candidates.is_empty() => {
            Some(candidates[rng.random_range(0..candidates.len())])
        }
        NeighborSelection::Random => None,
    }
}

fn apply_neighbor_updates(
    automaton: &AutomatonDefinition,
    world: &mut World,
    updates: Vec<(Position, NeighborPropertyUpdate)>,
) -> Result<(), String> {
    if updates.is_empty() {
        return Ok(());
    }
    let mut totals = BTreeMap::<(usize, usize, String), f64>::new();
    for (origin, update) in updates {
        let Some(target) = offset_destination(origin, update.offset, world, &automaton.boundary)
        else {
            continue;
        };
        let total = totals
            .entry((target.row, target.col, update.property))
            .or_default();
        *total += update.value;
        if !total.is_finite() {
            return Err(format!(
                "Cell ({}, {}): Neighbor contribution sum must be finite",
                target.row, target.col
            ));
        }
    }
    for ((row, col, property), delta) in totals {
        let cell = &mut world.cells[row][col];
        let value = cell.properties.get_mut(&property).ok_or_else(|| {
            format!("Cell ({row}, {col}): Missing neighbor contribution property '{property}'")
        })?;
        *value += delta;
    }
    for (row, cells) in world.cells.iter().enumerate() {
        for (col, cell) in cells.iter().enumerate() {
            automaton
                .validate_object_state(cell)
                .map_err(|error| format!("Cell ({row}, {col}): Neighbor contributions: {error}"))?;
        }
    }
    Ok(())
}

fn resolve_neighbor_removals(decisions: &mut [AgentDecision]) -> HashSet<u64> {
    let mut self_removed = HashSet::new();
    let mut requests = HashMap::<u64, usize>::new();
    for decision in decisions.iter() {
        if decision.result.remove_agent {
            self_removed.insert(decision.agent.id);
        }
        if let Some(target) = decision.result.neighbor_to_remove {
            *requests.entry(target).or_default() += 1;
        }
    }
    let mut removed = HashSet::new();
    for decision in decisions {
        if let Some(target) = decision.result.neighbor_to_remove {
            if self_removed.contains(&target) || requests[&target] > 1 {
                decision.result = ActionResult::unchanged(&decision.agent.state);
                decision.destination = decision.agent.position;
            } else {
                removed.insert(target);
            }
        }
    }
    removed
}

fn apply_cell_updates(
    automaton: &AutomatonDefinition,
    world: &mut World,
    updates: Vec<(Position, u64, CellUpdate)>,
) -> Result<(), String> {
    let mut writers = HashMap::new();
    for (position, agent_id, _) in &updates {
        if let Some(previous) = writers.insert(*position, *agent_id) {
            return Err(format!(
                "Cell ({}, {}): Conflicting cell updates from agents {previous} and {agent_id}",
                position.row, position.col
            ));
        }
    }
    for (position, agent_id, update) in updates {
        world.cells[position.row][position.col] = update
            .apply(&world.cells[position.row][position.col], automaton)
            .map_err(|error| {
                format!(
                    "Cell ({}, {}): Agent {agent_id}: Cell update: {error}",
                    position.row, position.col
                )
            })?;
    }
    Ok(())
}

fn offset_destination(
    position: Position,
    offset: (i32, i32),
    world: &World,
    boundary: &Boundary,
) -> Option<Position> {
    let height = world.height() as i64;
    let width = world.width() as i64;
    let row = position.row as i64 + i64::from(offset.0);
    let col = position.col as i64 + i64::from(offset.1);

    match boundary {
        Boundary::Wrap => Some(Position {
            row: row.rem_euclid(height) as usize,
            col: col.rem_euclid(width) as usize,
        }),
        Boundary::Fixed { .. } => {
            if row < 0 || col < 0 || row >= height || col >= width {
                None
            } else {
                Some(Position {
                    row: row as usize,
                    col: col as usize,
                })
            }
        }
    }
}

fn add_spawned_agents(
    automaton: &AutomatonDefinition,
    world: &mut World,
    spawns: Vec<(Position, AgentSpawn)>,
    last_agent_id: &mut u64,
) -> Result<(), String> {
    let mut candidates = Vec::new();
    for (origin, spawn) in spawns {
        if let Some(position) = offset_destination(origin, spawn.offset, world, &automaton.boundary)
        {
            candidates.push((position, spawn.state));
        }
    }
    let occupied: HashSet<_> = world.agents.iter().map(|agent| agent.position).collect();
    let mut requests = HashMap::<Position, usize>::new();
    for (position, _) in &candidates {
        *requests.entry(*position).or_default() += 1;
    }

    for (position, state) in candidates {
        if occupied.contains(&position) || requests[&position] > 1 {
            continue;
        }
        *last_agent_id = last_agent_id.checked_add(1).ok_or_else(|| {
            format!(
                "Spawn at ({}, {}): Agent ID range exhausted",
                position.row, position.col
            )
        })?;
        world.agents.push(Agent {
            id: *last_agent_id,
            position,
            state,
        });
    }
    Ok(())
}

fn resolve_moves(decisions: &mut [AgentDecision]) {
    let mut requests = HashMap::<Position, usize>::new();
    for decision in decisions.iter() {
        if decision.destination != decision.agent.position {
            *requests.entry(decision.destination).or_default() += 1;
        }
    }

    // All movers requesting the same destination stay at their original positions.
    for decision in decisions.iter_mut() {
        if requests.get(&decision.destination).copied().unwrap_or(0) > 1 {
            decision.destination = decision.agent.position;
        }
    }

    // A cancelled move can block a chain of other moves; repeat until none change.
    loop {
        let mut occupied = HashSet::new();
        for decision in decisions.iter() {
            if decision.destination == decision.agent.position {
                occupied.insert(decision.agent.position);
            }
        }
        let mut changed = false;
        for decision in decisions.iter_mut() {
            if decision.destination != decision.agent.position
                && occupied.contains(&decision.destination)
            {
                decision.destination = decision.agent.position;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

pub fn run(
    automaton: &AutomatonDefinition,
    initial: &World,
    generations: usize,
    seed: u64,
    save_history: bool,
) -> Result<Vec<SimulationGeneration>, String> {
    automaton
        .validate_world(initial)
        .map_err(|error| format!("Generation 0: {error}"))?;

    let mut rng = StdRng::seed_from_u64(seed);
    let mut last_agent_id = initial
        .agents
        .iter()
        .map(|agent| agent.id)
        .max()
        .unwrap_or(0);
    let max_properties = automaton
        .object_types
        .iter()
        .map(|object_type| object_type.properties.len())
        .max()
        .unwrap_or(0);
    let mut history_units = world_history_units(initial, max_properties)
        .filter(|count| *count <= MAX_HISTORY_UNITS)
        .ok_or_else(|| {
            format!(
                "Generation 0: Simulation history exceeds {MAX_HISTORY_UNITS} state/property units"
            )
        })?;
    let mut current = initial.clone();
    let mut history = Vec::new();

    for generation in 1..=generations {
        let next = step_with_ids(automaton, &current, &mut rng, &mut last_agent_id)
            .map_err(|error| format!("Generation {generation}: {error}"))?;

        let units = world_history_units(&next, max_properties)
            .ok_or_else(|| history_limit_error(generation))?;
        if save_history {
            history_units = history_units
                .checked_add(units)
                .ok_or_else(|| history_limit_error(generation))?;
        } else {
            history_units = units;
        }
        if history_units > MAX_HISTORY_UNITS {
            return Err(history_limit_error(generation));
        }
        if save_history {
            history.push(SimulationGeneration {
                generation: generation - 1,
                world: current,
            });
        }
        current = next;
    }

    history.push(SimulationGeneration {
        generation: generations,
        world: current,
    });

    Ok(history)
}

fn history_limit_error(generation: usize) -> String {
    format!(
        "Generation {generation}: Simulation history exceeds {MAX_HISTORY_UNITS} state/property units"
    )
}
