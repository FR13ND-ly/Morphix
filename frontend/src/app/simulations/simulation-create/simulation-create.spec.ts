import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { provideRouter, Router } from '@angular/router';
import { vi } from 'vitest';
import { SimulationCreate } from './simulation-create';
import { SimulationRequest } from '../simulation.models';

describe('SimulationCreate', () => {
  let component: SimulationCreate;
  let fixture: ComponentFixture<SimulationCreate>;
  let http: HttpTestingController;
  const request: SimulationRequest = {
    automaton: {
      name: 'Game of Life',
      object_types: [{ id: 0, name: 'Gol', properties: {} }],
      neighborhood: { offsets: [[0, 1]] },
      boundary: { kind: 'wrap' },
      rules: [],
    },
    initial_world: { cells: [[{ type_id: 0, properties: {} }]], agents: [] },
    generations: 10,
    seed: 42,
    save_history: true,
  };
  const startedRequest = { ...request, generations: 0, save_history: true };

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [SimulationCreate],
      providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([])],
    }).compileComponents();
    fixture = TestBed.createComponent(SimulationCreate);
    component = fixture.componentInstance;
    http = TestBed.inject(HttpTestingController);
  });

  afterEach(() => {
    http.verify();
    vi.restoreAllMocks();
  });

  function loadExample() {
    component.loadExample();
    http.expectOne('/api/simulations/examples/game-of-life').flush(request);
  }

  it('requires a loaded example and offers no file import or editor', () => {
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('input[type="file"]')).toBeNull();
    expect(fixture.nativeElement.querySelector('textarea')).toBeNull();
    expect(component.configuration()).toBeNull();
    component.run();
    component.validate();
    http.expectNone('/api/simulations/run');
    http.expectNone('/api/simulations/validate');
  });

  it('loads an example and shows its name and run parameters', () => {
    loadExample();
    fixture.detectChanges();
    expect(component.configuration()).toEqual(request);
    expect(fixture.nativeElement.querySelector('h2').textContent).toBe('Game of Life');
    expect(fixture.nativeElement.querySelector('#generations')).toBeNull();
    expect(fixture.nativeElement.textContent).toContain('pregătite în avans');
    expect(component.parameters.controls.randomize.value).toBe(true);
    expect(component.source()).toBe('Game of Life');
  });

  it('clears validation when another example is selected', () => {
    loadExample();
    component.validate();
    http.expectOne('/api/simulations/validate').flush({ valid: true });
    expect(component.validated()).toBe(true);
    component.example.setValue('diffusion');
    component.loadExample();
    expect(component.validated()).toBe(false);
    expect(component.configuration()).toBeNull();
    http.expectOne('/api/simulations/examples/diffusion').flush(request);
  });

  it('does not run the previous configuration if loading another example fails', () => {
    loadExample();
    component.example.setValue('diffusion');
    component.loadExample();
    http
      .expectOne('/api/simulations/examples/diffusion')
      .flush({ error: 'Example not found' }, { status: 404, statusText: 'Not Found' });
    component.run();
    http.expectNone('/api/simulations/run');
    expect(component.error()).toBe('Example not found');
    expect(component.configuration()).toBeNull();
  });

  it('prevents duplicate runs and opens the saved result', () => {
    loadExample();
    const navigate = vi.spyOn(TestBed.inject(Router), 'navigate').mockResolvedValue(true);
    component.run();
    component.run();
    const call = http.expectOne('/api/simulations/run');
    expect(call.request.body).toEqual(startedRequest);
    call.flush({ id: 9, seed: 42, history: [] });
    expect(navigate).toHaveBeenCalledWith(['/simulations', 9]);
  });

  it('keeps the loaded configuration after a runtime rejection', () => {
    loadExample();
    component.run();
    http
      .expectOne('/api/simulations/run')
      .flush({ error: 'Invalid generated value' }, { status: 422, statusText: 'Invalid' });
    expect(component.configuration()).toEqual(request);
    expect(component.error()).toBe('Invalid generated value');
    expect(component.busy()).toBeNull();
  });

  it('cancels validation when leaving the creation page', () => {
    loadExample();
    component.validate();
    const obsolete = http.expectOne('/api/simulations/validate');
    fixture.destroy();
    expect(obsolete.cancelled).toBe(true);
  });

  it('starts at generation zero with the edited seed and retains every future step', () => {
    loadExample();
    fixture.detectChanges();
    const seed: HTMLInputElement = fixture.nativeElement.querySelector('#seed');
    seed.value = '123';
    seed.dispatchEvent(new Event('input'));
    component.validate();
    const validation = http.expectOne('/api/simulations/validate');
    expect(validation.request.body).toEqual({
      ...request,
      generations: 0,
      seed: 123,
      save_history: true,
    });
    validation.flush({ valid: true });
    component.run();
    const run = http.expectOne('/api/simulations/run');
    expect(run.request.body).toEqual(validation.request.body);
    run.flush({ error: 'Temporary error' }, { status: 500, statusText: 'Error' });
    expect(request.generations).toBe(10);
    expect(request.seed).toBe(42);
  });

  it('generates a safe random seed through the button and clears previous validation', () => {
    loadExample();
    component.validate();
    http.expectOne('/api/simulations/validate').flush({ valid: true });
    const random = vi
      .spyOn(crypto, 'getRandomValues')
      .mockReturnValue(new Uint32Array([4000000000]));
    fixture.detectChanges();
    const button = Array.from(
      fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
    ).find((button) => button.textContent?.includes('Seed aleatoriu'))!;
    button.click();
    fixture.detectChanges();
    expect(random).toHaveBeenCalledOnce();
    expect(component.parameters.controls.seed.value).toBe(4000000000);
    expect(fixture.nativeElement.querySelector('#seed').value).toBe('4000000000');
    expect(component.validated()).toBe(false);
    component.validate();
    const validation = http.expectOne('/api/simulations/validate');
    expect(validation.request.body.seed).toBe(4000000000);
    validation.flush({ valid: true });
  });

  it.each([{ seed: -1 }, { seed: 0.5 }, { seed: Number.MAX_SAFE_INTEGER + 1 }])(
    'rejects invalid parameters before making a request: %j',
    (parameters) => {
      loadExample();
      component.parameters.patchValue(parameters);
      component.run();
      component.validate();
      http.expectNone('/api/simulations/run');
      http.expectNone('/api/simulations/validate');
      expect(component.error()).toContain('număr întreg');
    },
  );

  it('allows zero generations and the largest exactly represented seed', () => {
    loadExample();
    component.parameters.patchValue({ seed: Number.MAX_SAFE_INTEGER });
    component.validate();
    const validation = http.expectOne('/api/simulations/validate');
    expect(validation.request.body.generations).toBe(0);
    expect(validation.request.body.seed).toBe(Number.MAX_SAFE_INTEGER);
    validation.flush({ valid: true });
  });

  it('preserves the initial pattern and uses independent default states for added cells', () => {
    const original: SimulationRequest = {
      ...request,
      automaton: {
        ...request.automaton,
        object_types: [
          {
            id: 4,
            name: 'Temperatură',
            properties: {
              temperature: {
                default_value: 20.5,
                min_value: 0,
                max_value: 100,
                integer_only: false,
              },
            },
          },
        ],
      },
      initial_world: { cells: [[{ type_id: 4, properties: { temperature: 80 } }]], agents: [] },
    };
    component.loadExample();
    http.expectOne('/api/simulations/examples/game-of-life').flush(original);
    component.parameters.controls.randomize.setValue(false);
    component.validate();
    http.expectOne('/api/simulations/validate').flush({ valid: true });
    component.editGridSize();
    component.gridSize.setValue({ rows: 2, columns: 3 });
    component.applyGridSize();
    const cells = component.configuration()!.initial_world.cells;
    expect(cells.map((row) => row.length)).toEqual([3, 3]);
    expect(cells[0][0].properties['temperature']).toBe(80);
    expect(cells[0][1]).toEqual({ type_id: 4, properties: { temperature: 20.5 } });
    expect(cells[0][1]).not.toBe(cells[0][2]);
    expect(cells[0][1].properties).not.toBe(cells[1][1].properties);
    expect(original.initial_world.cells).toEqual([
      [{ type_id: 4, properties: { temperature: 80 } }],
    ]);
    expect(component.editingGrid()).toBe(false);
    expect(component.validated()).toBe(false);
    component.validate();
    const validation = http.expectOne('/api/simulations/validate');
    expect(validation.request.body.initial_world.cells).toEqual(cells);
    validation.flush({ valid: true });
  });

  it('refuses shrinking past agents and preserves every agent when resizing succeeds', () => {
    const agents = [{ id: 7, position: { row: 0, col: 2 }, state: { type_id: 0, properties: {} } }];
    component.loadExample();
    http.expectOne('/api/simulations/examples/game-of-life').flush({
      ...request,
      initial_world: {
        cells: [
          [
            { type_id: 0, properties: {} },
            { type_id: 0, properties: {} },
            { type_id: 0, properties: {} },
          ],
        ],
        agents,
      },
    });
    const original = component.configuration();
    component.editGridSize();
    component.gridSize.setValue({ rows: 1, columns: 2 });
    component.applyGridSize();
    expect(component.configuration()).toBe(original);
    expect(component.editingGrid()).toBe(true);
    expect(component.error()).toContain('ar elimina agenți');
    component.gridSize.setValue({ rows: 2, columns: 3 });
    component.applyGridSize();
    expect(component.configuration()!.initial_world.agents).toEqual(agents);
    component.editGridSize();
    component.gridSize.setValue({ rows: 1, columns: 3 });
    component.applyGridSize();
    expect(component.configuration()!.initial_world.cells.length).toBe(1);
    expect(component.configuration()!.initial_world.agents).toEqual(agents);
  });

  it('cancels pending dimensions without changing the configuration or running it', () => {
    loadExample();
    const original = component.configuration();
    component.editGridSize();
    component.gridSize.setValue({ rows: 10, columns: 10 });
    component.run();
    component.validate();
    http.expectNone('/api/simulations/run');
    http.expectNone('/api/simulations/validate');
    component.cancelGridSize();
    expect(component.configuration()).toBe(original);
    expect(component.editingGrid()).toBe(false);
  });

  it.each([0, -1, 1.5, 100001])(
    'rejects invalid grid dimension %s without replacing the world',
    (rows) => {
      loadExample();
      const original = component.configuration();
      component.editGridSize();
      component.gridSize.setValue({ rows, columns: 2 });
      component.applyGridSize();
      expect(component.configuration()).toBe(original);
      expect(component.error()).toContain('numere întregi pozitive');
    },
  );

  it('limits one world before allocation without reserving a fixed number of generations', () => {
    loadExample();
    const original = component.configuration();
    component.editGridSize();
    component.gridSize.setValue({ rows: 100000, columns: 100000 });
    component.applyGridSize();
    expect(component.configuration()).toBe(original);
    expect(component.error()).toContain('limita de dimensiune');
    component.gridSize.setValue({ rows: 100, columns: 100 });
    component.applyGridSize();
    expect(component.configuration()!.initial_world.cells.length).toBe(100);
    expect(component.configuration()!.initial_world.cells[0].length).toBe(100);
    expect(component.error()).toBe('');
  });

  it('resets edited parameters and dimensions when loading another example', () => {
    loadExample();
    component.parameters.patchValue({ seed: 789 });
    component.editGridSize();
    component.example.setValue('diffusion');
    component.loadExample();
    http.expectOne('/api/simulations/examples/diffusion').flush(request);
    expect(component.parameters.getRawValue()).toEqual({
      seed: 42,
      randomize: true,
    });
    expect(component.configuration()).toEqual(request);
    expect(component.editingGrid()).toBe(false);
  });

  it('uses the seed for repeatable generated worlds and sends the exact validated world when starting', () => {
    const original: SimulationRequest = {
      ...request,
      automaton: {
        ...request.automaton,
        object_types: [
          {
            id: 0,
            name: 'Teren',
            properties: {
              heat: { default_value: 0, min_value: 0, max_value: 100, integer_only: true },
            },
          },
        ],
      },
      initial_world: {
        cells: [0, 1, 2].map((row) =>
          [0, 1, 2].map((col) => ({ type_id: 0, properties: { heat: row * 3 + col } })),
        ),
        agents: [
          { id: 7, position: { row: 0, col: 1 }, state: { type_id: 0, properties: { heat: 100 } } },
          { id: 8, position: { row: 2, col: 2 }, state: { type_id: 0, properties: { heat: 50 } } },
        ],
      },
    };
    component.loadExample();
    http.expectOne('/api/simulations/examples/game-of-life').flush(original);
    function prepared(seed: number) {
      component.parameters.controls.seed.setValue(seed);
      component.validate();
      const validation = http.expectOne('/api/simulations/validate');
      validation.flush({ valid: true });
      return validation.request.body as SimulationRequest;
    }
    const first = prepared(42);
    const different = prepared(123);
    const repeated = prepared(42);
    expect(first.initial_world).toEqual(repeated.initial_world);
    expect(first.initial_world).not.toEqual(different.initial_world);
    expect(
      first.initial_world.cells
        .flat()
        .every(
          (cell) =>
            Number.isInteger(cell.properties['heat']) &&
            cell.properties['heat'] >= 0 &&
            cell.properties['heat'] <= 100,
        ),
    ).toBe(true);
    expect(first.initial_world.agents.length).toBe(2);
    for (const agent of first.initial_world.agents) {
      const terrain = first.initial_world.cells[agent.position.row][agent.position.col];
      expect(terrain.type_id).toBe(0);
    }
    expect(component.configuration()).toEqual(original);
    component.run();
    const run = http.expectOne('/api/simulations/run');
    expect(run.request.body).toEqual(repeated);
    expect(run.request.body).not.toHaveProperty('randomize');
    run.flush({ error: 'Temporary error' }, { status: 500, statusText: 'Error' });
    component.parameters.controls.randomize.setValue(false);
    const unchanged = prepared(123);
    expect(unchanged.initial_world).toEqual(original.initial_world);
  });

  it('applies initialization controls and rejects all-zero weights before HTTP validation', () => {
    loadExample();
    fixture.detectChanges();
    const weight: HTMLInputElement = fixture.nativeElement.querySelector('#weight-0');
    weight.value = '0';
    weight.dispatchEvent(new Event('input'));
    component.validate();
    http.expectNone('/api/simulations/validate');
    expect(component.error()).toContain('Cel puțin un tip');
    component.parameters.controls.randomize.setValue(false);
    component.validate();
    http.expectOne('/api/simulations/validate').flush({ valid: true });
  });
});
