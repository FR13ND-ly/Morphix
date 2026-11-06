import { TestBed } from '@angular/core/testing';
import { WorldInitializer } from './world-initializer';
import { SimulationRequest } from './simulation.models';

describe('WorldInitializer', () => {
  let service: WorldInitializer;

  beforeEach(() => {
    TestBed.configureTestingModule({});
    service = TestBed.inject(WorldInitializer);
  });

  function request(): SimulationRequest {
    return {
      automaton: {
        name: 'Model generic',
        object_types: [
          { id: 10, name: 'Teren', properties: {} },
          { id: 20, name: 'Alt teren', properties: {} },
          {
            id: 30,
            name: 'Agent',
            properties: {
              energy: { default_value: 8, min_value: 0, max_value: 100, integer_only: true },
            },
          },
        ],
        neighborhood: { offsets: [[0, 1]] },
        boundary: { kind: 'wrap' },
        rules: [],
      },
      initial_world: {
        cells: Array.from({ length: 4 }, (_, row) =>
          Array.from({ length: 5 }, (_, col) => ({
            type_id: row === 0 && col < 3 ? 20 : 10,
            properties: {},
          })),
        ),
        agents: [
          {
            id: 99,
            position: { row: 2, col: 2 },
            state: { type_id: 30, properties: { energy: 5 } },
          },
        ],
      },
      seed: 42,
      generations: 0,
      save_history: true,
    };
  }

  it('derives weights, agent counts, terrain and property ranges from arbitrary declared IDs', () => {
    const settings = service.settings(request());
    expect(settings.map((type) => type.cell_weight)).toEqual([85, 15, 0]);
    expect(settings.map((type) => type.agent_count)).toEqual([0, 0, 1]);
    expect(settings[2].agent_cell_type).toBe(10);
    expect(settings[2].properties[0].minimum).toBe(5);
    expect(settings[2].properties[0].maximum).toBe(5);
  });

  it('generates variable counts from the seed rather than permuting the original cells', () => {
    const source = request();
    const snapshot = structuredClone(source);
    const settings = service.settings(source);
    settings[2].agent_count = 0;
    const worlds = [1, 2, 3, 42, 123].map((seed) => service.generate(source, settings, seed));
    const counts = worlds.map(
      (world) => world.cells.flat().filter((cell) => cell.type_id === 20).length,
    );
    expect(new Set(counts).size).toBeGreaterThan(1);
    expect(worlds[3]).toEqual(service.generate(source, settings, 42));
    expect(source).toEqual(snapshot);
    expect(worlds.every((world) => world.agents.length === 0)).toBe(true);
  });

  it('normalizes relative weights and respects zero and full probability', () => {
    const source = request();
    const settings = service.settings(source);
    settings[2].agent_count = 0;
    settings[0].cell_weight = 7;
    settings[1].cell_weight = 3;
    const first = service.generate(source, settings, 42);
    settings[0].cell_weight = 70;
    settings[1].cell_weight = 30;
    expect(service.generate(source, settings, 42)).toEqual(first);
    settings[0].cell_weight = 0;
    settings[1].cell_weight = 100;
    expect(
      service
        .generate(source, settings, 42)
        .cells.flat()
        .every((cell) => cell.type_id === 20),
    ).toBe(true);
  });

  it('creates declared types absent from the example using their property defaults', () => {
    const source = request();
    source.initial_world.agents = [];
    const settings = service.settings(source);
    settings.forEach((type) => (type.cell_weight = type.type_id === 30 ? 100 : 0));
    const world = service.generate(source, settings, 42);
    expect(
      world.cells.flat().every((cell) => cell.type_id === 30 && cell.properties['energy'] === 8),
    ).toBe(true);
    expect(world.cells[0][0].properties).not.toBe(world.cells[0][1].properties);
  });

  it('samples decimal and integer properties within their configured bounds', () => {
    const source = request();
    source.automaton.object_types[0].properties = {
      heat: { default_value: 0, min_value: -100, max_value: 100, integer_only: false },
      grains: { default_value: 0, min_value: -10, max_value: 10, integer_only: true },
    };
    source.initial_world.cells.forEach((row) =>
      row.forEach((cell) => {
        if (cell.type_id === 10) cell.properties = { heat: 0, grains: 0 };
      }),
    );
    const settings = service.settings(source);
    settings[0].cell_weight = 100;
    settings[1].cell_weight = 0;
    settings[2].agent_count = 0;
    settings[0].properties[0].minimum = -2.5;
    settings[0].properties[0].maximum = 3.5;
    settings[0].properties[1].minimum = -3;
    settings[0].properties[1].maximum = 3;
    const world = service.generate(source, settings, 42);
    for (const cell of world.cells.flat()) {
      expect(cell.properties['heat']).toBeGreaterThanOrEqual(-2.5);
      expect(cell.properties['heat']).toBeLessThanOrEqual(3.5);
      expect(Number.isInteger(cell.properties['grains'])).toBe(true);
      expect(Math.abs(cell.properties['grains'])).toBeLessThanOrEqual(3);
    }
    expect(world.cells.flat().some((cell) => !Number.isInteger(cell.properties['heat']))).toBe(
      true,
    );
    settings[0].properties[0].minimum = 1.25;
    settings[0].properties[0].maximum = 1.25;
    expect(service.generate(source, settings, 42).cells[0][0].properties['heat']).toBe(1.25);
  });

  it('generates requested agent counts with unique positions, IDs and permitted terrain', () => {
    const source = request();
    const settings = service.settings(source);
    settings[0].cell_weight = 100;
    settings[1].cell_weight = 0;
    settings[1].agent_count = 2;
    settings[2].agent_count = 3;
    const world = service.generate(source, settings, 42);
    expect(world.agents.length).toBe(5);
    expect(world.agents.filter((agent) => agent.state.type_id === 30).length).toBe(3);
    expect(new Set(world.agents.map((agent) => agent.id)).size).toBe(5);
    expect(
      new Set(world.agents.map((agent) => `${agent.position.row},${agent.position.col}`)).size,
    ).toBe(5);
    expect(
      world.agents.every(
        (agent) => world.cells[agent.position.row][agent.position.col].type_id === 10,
      ),
    ).toBe(true);
    expect(service.generate(source, settings, 42)).toEqual(world);
  });

  it('rejects impossible placement and invalid probabilities, counts or ranges', () => {
    const source = request();
    let settings = service.settings(source);
    settings[0].cell_weight = 0;
    settings[1].cell_weight = 100;
    expect(() => service.generate(source, settings, 42)).toThrow('Nu sunt suficiente');
    settings = service.settings(source);
    settings.forEach((type) => (type.cell_weight = 0));
    expect(() => service.generate(source, settings, 42)).toThrow('Cel puțin un tip');
    settings[0].cell_weight = -1;
    expect(() => service.generate(source, settings, 42)).toThrow('Ponderea');
    settings = service.settings(source);
    settings[2].agent_count = 0.5;
    expect(() => service.generate(source, settings, 42)).toThrow('întreg');
    settings[2].agent_count = 21;
    expect(() => service.generate(source, settings, 42)).toThrow('depășește numărul celulelor');
    settings = service.settings(source);
    settings[2].properties[0].minimum = 8;
    settings[2].properties[0].maximum = 3;
    expect(() => service.generate(source, settings, 42)).toThrow('Interval invalid');
    settings[2].properties[0].minimum = -1;
    settings[2].properties[0].maximum = 3;
    expect(() => service.generate(source, settings, 42)).toThrow('Interval invalid');
    settings[2].properties[0].minimum = 0.5;
    expect(() => service.generate(source, settings, 42)).toThrow('întregi');
  });
});
