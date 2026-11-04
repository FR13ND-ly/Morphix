use std::collections::{BTreeMap, HashMap, HashSet};

use rand::{Rng, RngExt, SeedableRng, rngs::StdRng};

use crate::models::{
    action::{
        ActionResult, AgentSpawn, CellUpdate, FreeCellMove, NeighborPropertyUpdate,
        NeighborSelection,
    },
    agent::Agent,
    automaton::{AutomatonDefinition, Boundary},
    position::Position,
    rule::{RuleTarget, apply_rules},
    simulation::{MAX_HISTORY_UNITS, SimulationGeneration, world_history_units},
    world::World,
};

pub fn step(
    automaton: &AutomatonDefinition,
    current: &World,
    rng: &mut impl Rng,
) -> Result<World, String> {
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
    automaton.validate_world(current)?;

    let mut next = current.clone();
    let mut spawns = Vec::new();
    let mut cell_updates = Vec::new();
    let mut neighbor_updates = Vec::new();

    for (row, cells) in current.cells.iter().enumerate() {
        for (col, state) in cells.iter().enumerate() {
            let neighbors = automaton
                .neighborhood
                .read(current, Position { row, col }, &automaton.boundary)
                .map_err(|error| format!("Cell ({row}, {col}): {error}"))?;

            let result = apply_rules(RuleTarget::Cell, state, state, &neighbors, automaton, rng)
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
    let mut relocations = Vec::with_capacity(current.agents.len());
    let occupied: HashSet<_> = current.agents.iter().map(|agent| agent.position).collect();
    for agent in &current.agents {
        let neighbors = automaton
            .neighborhood
            .read(current, agent.position, &automaton.boundary)
            .map_err(|error| format!("Agent {}: {error}", agent.id))?;

        let cell = &current.cells[agent.position.row][agent.position.col];
        let result = apply_rules(
            RuleTarget::Agent,
            &agent.state,
            cell,
            &neighbors,
            automaton,
            rng,
        )
        .map_err(|error| format!("Agent {}: {error}", agent.id))?;
        relocations.push(
            result
                .free_cell_move
                .and_then(|request| choose_free_cell(current, &occupied, request, rng)),
        );
        decisions.push(result);
    }

    let removed_neighbors = resolve_neighbor_removals(&current.agents, &mut decisions);
    next.agents.clear();
    let mut destinations = Vec::with_capacity(current.agents.len());
    for ((agent, result), relocation) in current.agents.iter().zip(decisions).zip(relocations) {
        if result.remove_agent || removed_neighbors.contains(&agent.id) {
            continue;
        }
        if let Some(update) = result.cell_update {
            cell_updates.push((agent.position, agent.id, update));
        }
        if let Some(spawn) = result.spawn {
            spawns.push((agent.position, spawn));
        }

        let mut next_agent = agent.clone();
        next_agent.state = result.state;
        next.agents.push(next_agent);
        let destination = if result.free_cell_move.is_some() {
            relocation.unwrap_or(agent.position)
        } else if result.move_to_removed_neighbor {
            current
                .agents
                .iter()
                .find(|target| Some(target.id) == result.neighbor_to_remove)
                .map(|target| target.position)
                .unwrap_or(agent.position)
        } else {
            match result.movement {
                Some(offset) => {
                    offset_destination(agent.position, offset, current, &automaton.boundary)
                        .unwrap_or(agent.position)
                }
                None => agent.position,
            }
        };
        destinations.push(destination);
    }

    apply_cell_updates(automaton, &mut next, cell_updates)?;
    apply_neighbor_updates(automaton, &mut next, neighbor_updates)?;

    // Survivors still have their original positions when movement is resolved.
    resolve_moves(&next.agents, &mut destinations);
    for (agent, destination) in next.agents.iter_mut().zip(destinations) {
        agent.position = destination;
    }
    add_spawned_agents(automaton, &mut next, spawns, last_agent_id)?;

    Ok(next)
}

fn choose_free_cell(
    world: &World,
    occupied: &HashSet<Position>,
    request: FreeCellMove,
    rng: &mut impl Rng,
) -> Option<Position> {
    let candidates: Vec<_> = world
        .cells
        .iter()
        .enumerate()
        .flat_map(|(row, cells)| {
            cells.iter().enumerate().filter_map(move |(col, cell)| {
                let position = Position { row, col };
                (!occupied.contains(&position)
                    && request.cell_type.is_none_or(|id| cell.type_id == id))
                .then_some(position)
            })
        })
        .collect();
    match request.selection {
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

fn resolve_neighbor_removals(agents: &[Agent], decisions: &mut [ActionResult]) -> HashSet<u64> {
    let self_removed: HashSet<_> = agents
        .iter()
        .zip(decisions.iter())
        .filter(|(_, result)| result.remove_agent)
        .map(|(agent, _)| agent.id)
        .collect();
    let mut requests = HashMap::<u64, usize>::new();
    for result in decisions.iter() {
        if let Some(target) = result.neighbor_to_remove {
            *requests.entry(target).or_default() += 1;
        }
    }
    let mut removed = HashSet::new();
    for (agent, result) in agents.iter().zip(decisions.iter_mut()) {
        if let Some(target) = result.neighbor_to_remove {
            if self_removed.contains(&target) || requests[&target] > 1 {
                *result = ActionResult::unchanged(&agent.state);
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
    let candidates: Vec<_> = spawns
        .into_iter()
        .filter_map(|(origin, spawn)| {
            offset_destination(origin, spawn.offset, world, &automaton.boundary)
                .map(|position| (position, spawn.state))
        })
        .collect();
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

fn resolve_moves(agents: &[Agent], destinations: &mut [Position]) {
    let mut requests = HashMap::<Position, usize>::new();
    for (agent, destination) in agents.iter().zip(destinations.iter()) {
        if *destination != agent.position {
            *requests.entry(*destination).or_default() += 1;
        }
    }

    // All movers requesting the same destination stay at their original positions.
    for (agent, destination) in agents.iter().zip(destinations.iter_mut()) {
        if requests.get(destination).copied().unwrap_or(0) > 1 {
            *destination = agent.position;
        }
    }

    // A cancelled move can block a chain of other moves; repeat until none change.
    loop {
        let occupied: HashSet<Position> = agents
            .iter()
            .zip(destinations.iter())
            .filter(|(agent, destination)| agent.position == **destination)
            .map(|(agent, _)| agent.position)
            .collect();
        let mut changed = false;
        for (agent, destination) in agents.iter().zip(destinations.iter_mut()) {
            if *destination != agent.position && occupied.contains(destination) {
                *destination = agent.position;
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

        history_units = world_history_units(&next, max_properties)
            .and_then(|units| {
                if save_history {
                    history_units.checked_add(units)
                } else {
                    Some(units)
                }
            })
            .filter(|count| *count <= MAX_HISTORY_UNITS)
            .ok_or_else(|| format!("Generation {generation}: Simulation history exceeds {MAX_HISTORY_UNITS} state/property units"))?;
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
