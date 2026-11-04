import { TestBed, ComponentFixture } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { Simulator } from './simulator';
import { SimulationRequest } from '../simulation.models';
import axe from 'axe-core';

const EXAMPLE: SimulationRequest = {
  automaton: {
    name: 'Game of Life',
    object_types: [
      { id: 0, name: 'dead', properties: {} },
      { id: 1, name: 'alive', properties: {} },
    ],
    neighborhood: { offsets: [[0, 1]] },
    boundary: { kind: 'wrap' },
    rules: [],
  },
  initial_world: {
    cells: [
      [
        { type_id: 1, properties: {} },
        { type_id: 0, properties: {} },
      ],
    ],
    agents: [],
  },
  generations: 2,
  seed: 42,
};

describe('Simulator', () => {
  let fixture: ComponentFixture<Simulator>;
  let http: HttpTestingController;
  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [Simulator],
      providers: [provideHttpClient(), provideHttpClientTesting()],
    }).compileComponents();
    fixture = TestBed.createComponent(Simulator);
    http = TestBed.inject(HttpTestingController);
    fixture.detectChanges();
    http.expectOne('/api/health').flush('ok');
    http.expectOne('/api/simulations/examples/game-of-life').flush(EXAMPLE);
    await fixture.whenStable();
    fixture.detectChanges();
  });
  afterEach(() => http.verify());

  it('loads an example and renders its actual world', () => {
    expect(fixture.componentInstance.connected()).toBe(true);
    expect(fixture.componentInstance.cellCount()).toBe(2);
    expect((fixture.nativeElement as HTMLElement).querySelectorAll('.cell').length).toBe(2);
  });

  it('has accessible names and valid ARIA attributes on controls', async () => {
    const report = await axe.run(fixture.nativeElement as HTMLElement, {
      runOnly: {
        type: 'rule',
        values: ['label', 'button-name', 'aria-allowed-attr', 'aria-prohibited-attr'],
      },
    });
    expect(report.violations).toEqual([]);
  });

  it('validates then runs the chosen settings and navigates the returned frames', async () => {
    const component = fixture.componentInstance;
    component.settings.setValue({ generations: 1, seed: 7, saveHistory: true });
    const promise = component.run();
    const validation = http.expectOne('/api/simulations/validate');
    expect(validation.request.body.generations).toBe(1);
    expect(validation.request.body.seed).toBe(7);
    validation.flush({ valid: true });
    await Promise.resolve();
    const run = http.expectOne('/api/simulations/run');
    expect(run.request.body).toEqual(validation.request.body);
    const nextWorld = {
      cells: [
        [
          { type_id: 0, properties: {} },
          { type_id: 1, properties: {} },
        ],
      ],
      agents: [],
    };
    run.flush({
      id: 12,
      seed: 7,
      history: [
        { generation: 0, world: EXAMPLE.initial_world },
        { generation: 1, world: nextWorld },
      ],
    });
    await promise;
    component.goToFrame(1);
    expect(component.generation()).toBe(1);
    expect(component.world()).toEqual(nextWorld);
    component.goToFrame(0);
    expect(component.world()).toEqual(EXAMPLE.initial_world);
    expect(component.result()?.id).toBe(12);
  });

  it('shows a validation error without starting a run', async () => {
    const promise = fixture.componentInstance.run();
    http
      .expectOne('/api/simulations/validate')
      .flush(
        { error: 'Generation 0: Unknown object type' },
        { status: 422, statusText: 'Invalid configuration' },
      );
    await promise;
    expect(fixture.componentInstance.error()).toContain('Unknown object type');
    expect(fixture.componentInstance.running()).toBe(false);
    http.expectNone('/api/simulations/run');
  });

  it('rejects invalid settings before making a request', async () => {
    fixture.componentInstance.settings.controls.generations.setValue(1001);
    await fixture.componentInstance.run();
    http.expectNone('/api/simulations/validate');
    http.expectNone('/api/simulations/run');
  });

  it('uses the explicit generation number for final-only results', async () => {
    const component = fixture.componentInstance;
    component.settings.setValue({ generations: 10, seed: 42, saveHistory: false });
    const promise = component.run();
    http.expectOne('/api/simulations/validate').flush({ valid: true });
    await Promise.resolve();
    const run = http.expectOne('/api/simulations/run');
    expect(run.request.body.save_history).toBe(false);
    run.flush({ id: 1, seed: 42, history: [{ generation: 10, world: EXAMPLE.initial_world }] });
    await promise;
    expect(component.generation()).toBe(10);
    expect(component.lastFrame()).toBe(0);
    component.togglePlayback();
    expect(component.playing()).toBe(false);
  });
});
