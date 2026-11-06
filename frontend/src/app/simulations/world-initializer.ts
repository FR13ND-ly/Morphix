import { Injectable } from '@angular/core';
import {
  Agent,
  InitialTypeSettings,
  ObjectState,
  SimulationRequest,
  World,
} from './simulation.models';

@Injectable({ providedIn: 'root' })
export class WorldInitializer {
  settings(request: SimulationRequest): InitialTypeSettings[] {
    const cells = request.initial_world.cells.flat();
    const agents = request.initial_world.agents;
    return request.automaton.object_types.map((type) => {
      const typeCells = cells.filter((cell) => cell.type_id === type.id);
      const typeAgents = agents.filter((agent) => agent.state.type_id === type.id);
      const states = [...typeCells, ...typeAgents.map((agent) => agent.state)];
      const terrainTypes = new Set(
        typeAgents.map(
          (agent) => request.initial_world.cells[agent.position.row][agent.position.col].type_id,
        ),
      );
      return {
        type_id: type.id,
        name: type.name,
        cell_weight: Number(((typeCells.length / cells.length) * 100).toFixed(6)),
        agent_count: typeAgents.length,
        agent_cell_type: terrainTypes.size === 1 ? [...terrainTypes][0] : null,
        properties: Object.entries(type.properties).map(([name, definition]) => {
          const values = states.map((state) => state.properties[name]);
          return {
            name,
            definition,
            minimum: values.length ? Math.min(...values) : definition.default_value,
            maximum: values.length ? Math.max(...values) : definition.default_value,
          };
        }),
      };
    });
  }

  generate(request: SimulationRequest, settings: InitialTypeSettings[], seed: number): World {
    const rows = request.initial_world.cells.length;
    const columns = request.initial_world.cells[0].length;
    const capacity = rows * columns;
    let weightSum = 0;
    let agentCount = 0;
    for (const type of settings) {
      const weight = type.cell_weight;
      const count = type.agent_count;
      if (weight === null || !Number.isFinite(weight) || weight < 0 || weight > 100) {
        throw new Error(`Ponderea pentru „${type.name}” trebuie să fie între 0 și 100.`);
      }
      if (count === null || !Number.isSafeInteger(count) || count < 0) {
        throw new Error(
          `Numărul agenților „${type.name}” trebuie să fie un întreg pozitiv sau zero.`,
        );
      }
      weightSum += weight;
      agentCount += count;
      if (weight === 0 && count === 0) continue;
      for (const property of type.properties) {
        const { minimum, maximum, definition } = property;
        if (
          minimum === null ||
          maximum === null ||
          !Number.isFinite(minimum) ||
          !Number.isFinite(maximum) ||
          minimum > maximum ||
          minimum < definition.min_value ||
          maximum > definition.max_value
        ) {
          throw new Error(
            `Interval invalid pentru „${type.name}.${property.name}”: respectă limitele definite și minim ≤ maxim.`,
          );
        }
        if (
          definition.integer_only &&
          (!Number.isSafeInteger(minimum) ||
            !Number.isSafeInteger(maximum) ||
            !Number.isSafeInteger(maximum - minimum + 1))
        ) {
          throw new Error(
            `„${type.name}.${property.name}” necesită un interval de întregi reprezentabili exact.`,
          );
        }
      }
    }
    if (weightSum <= 0)
      throw new Error('Cel puțin un tip de celulă trebuie să aibă pondere mai mare decât zero.');
    if (agentCount > capacity)
      throw new Error(
        'Numărul total al agenților depășește numărul celulelor. Mărește grila sau redu numărul agenților.',
      );
    const maxProperties = Math.max(
      0,
      ...request.automaton.object_types.map((type) => Object.keys(type.properties).length),
    );
    if ((capacity + agentCount) * (maxProperties + 1) > 100000) {
      throw new Error('Lumea generată depășește limita de dimensiune a simulatorului.');
    }

    let randomState = (seed >>> 0) ^ Math.floor(seed / 4294967296);
    const random = () => {
      randomState = (Math.imul(1664525, randomState) + 1013904223) >>> 0;
      return randomState / 4294967296;
    };
    const state = (type: InitialTypeSettings): ObjectState => ({
      type_id: type.type_id,
      properties: Object.fromEntries(
        type.properties.map((property) => {
          const minimum = property.minimum!;
          const maximum = property.maximum!;
          let value = minimum;
          if (minimum !== maximum) {
            const fraction = random();
            value = property.definition.integer_only
              ? minimum + Math.floor(fraction * (maximum - minimum + 1))
              : minimum * (1 - fraction) + maximum * fraction;
          }
          return [property.name, value];
        }),
      ),
    });
    const cellTypes = settings.filter((type) => (type.cell_weight ?? 0) > 0);
    const cells = Array.from({ length: rows }, () =>
      Array.from({ length: columns }, () => {
        let choice = random() * weightSum;
        let selected = cellTypes[cellTypes.length - 1];
        for (const type of cellTypes) {
          choice -= type.cell_weight!;
          if (choice < 0) {
            selected = type;
            break;
          }
        }
        return state(selected);
      }),
    );

    let available = Array.from({ length: capacity }, (_, index) => index);
    for (let index = available.length - 1; index > 0; index--) {
      const other = Math.floor(random() * (index + 1));
      [available[index], available[other]] = [available[other], available[index]];
    }
    const agents: Agent[] = [];
    // Place constrained agents first, so unrestricted agents do not take their terrain.
    const agentTypes = settings
      .filter((type) => (type.agent_count ?? 0) > 0)
      .sort(
        (left, right) =>
          Number(left.agent_cell_type === null) - Number(right.agent_cell_type === null),
      );
    for (const type of agentTypes) {
      const count = type.agent_count!;
      const eligible = available.filter(
        (index) =>
          type.agent_cell_type === null ||
          cells[Math.floor(index / columns)][index % columns].type_id === type.agent_cell_type,
      );
      if (eligible.length < count) {
        throw new Error(
          `Nu sunt suficiente celule permise pentru agenții „${type.name}”. Ajustează ponderile, numărul agenților sau terenul permis.`,
        );
      }
      const occupied = new Set(eligible.slice(0, count));
      for (const index of occupied) {
        agents.push({
          id: agents.length + 1,
          position: { row: Math.floor(index / columns), col: index % columns },
          state: state(type),
        });
      }
      available = available.filter((index) => !occupied.has(index));
    }
    return { cells, agents };
  }
}
