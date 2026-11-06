import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { ActivatedRoute, convertToParamMap, provideRouter, Router } from '@angular/router';
import { BehaviorSubject } from 'rxjs';
import { vi } from 'vitest';
import { StoredSimulation } from '../simulation.models';
import { SimulationDetails } from './simulation-details';

describe('SimulationDetails', () => {
  let component: SimulationDetails;
  let fixture: ComponentFixture<SimulationDetails>;
  let http: HttpTestingController;
  let params: BehaviorSubject<ReturnType<typeof convertToParamMap>>;
  let saved: StoredSimulation;

  beforeEach(async () => {
    params = new BehaviorSubject(convertToParamMap({ id: '7' }));
    saved = {
      id: 7,
      created_at: '2026-11-04T00:00:00Z',
      request: {
        automaton: {
          name: 'Test',
          object_types: [{ id: 0, name: 'Gol', properties: {} }],
          neighborhood: { offsets: [[0, 1]] },
          boundary: { kind: 'wrap' },
          rules: [],
        },
        initial_world: { cells: [[{ type_id: 0, properties: {} }]], agents: [] },
        generations: 1,
        seed: 42,
        save_history: true,
      },
      history: [0, 1].map((generation) => ({
        generation,
        world: { cells: [[{ type_id: 0, properties: {} }]], agents: [] },
      })),
    };
    await TestBed.configureTestingModule({
      imports: [SimulationDetails],
      providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([])],
    }).compileComponents();

    vi.spyOn(TestBed.inject(ActivatedRoute), 'paramMap', 'get').mockReturnValue(
      params.asObservable(),
    );
    fixture = TestBed.createComponent(SimulationDetails);
    component = fixture.componentInstance;
    http = TestBed.inject(HttpTestingController);
  });

  afterEach(() => {
    fixture.destroy();
    http.verify({ ignoreCancelled: true });
    vi.restoreAllMocks();
  });

  it('shows the saved generation number when only the final frame is retained', () => {
    saved.request.generations = 12;
    saved.request.save_history = false;
    saved.history = [{ generation: 12, world: saved.request.initial_world }];
    http.expectOne('/api/simulations/7').flush(saved);
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('h2').textContent).toBe('Generația 12');
    const controls: NodeListOf<HTMLButtonElement> =
      fixture.nativeElement.querySelectorAll('nav button');
    expect(controls[0].disabled).toBe(true);
    expect(controls[1].disabled).toBe(false);
    expect(fixture.nativeElement.textContent).toContain(
      'Generațiile intermediare nu au fost păstrate',
    );
  });

  it('navigates the retained history without changing stored worlds', () => {
    http.expectOne('/api/simulations/7').flush(saved);
    component.selectFrame(1);
    expect(component.generation()?.generation).toBe(1);
    component.selectFrame(20);
    expect(component.generation()?.generation).toBe(1);
    component.selectFrame(0);
    expect(component.generation()?.generation).toBe(0);
    expect(component.simulation()?.history).toEqual(saved.history);
  });

  it('inspects selected cells and displays agent properties', () => {
    saved.history[0].world.agents = [
      {
        id: 2,
        position: { row: 0, col: 0 },
        state: { type_id: 0, properties: {} },
      },
    ];
    http.expectOne('/api/simulations/7').flush(saved);
    fixture.detectChanges();
    fixture.nativeElement.querySelector('button[aria-label="Celula 0, 0: Gol"]').click();
    fixture.detectChanges();
    expect(component.inspectedState()?.type_id).toBe(0);
    expect(fixture.nativeElement.textContent).toContain('Celula (0, 0)');
    expect(fixture.nativeElement.textContent).toContain('Agenți: 1');
    expect(fixture.nativeElement.querySelector('a[href="/simulations/new?copyFrom=7"]')).toBeNull();
  });

  it('shows missing simulations and can load another ID on the same page', () => {
    http
      .expectOne('/api/simulations/7')
      .flush({ error: 'Simulation not found' }, { status: 404, statusText: 'Not Found' });
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('[role="alert"]').textContent).toContain(
      'Simulation not found',
    );
    params.next(convertToParamMap({ id: '8' }));
    http.expectOne('/api/simulations/8').flush({ ...saved, id: 8 });
    expect(component.simulation()?.id).toBe(8);
    expect(component.error()).toBe('');
  });

  it('moves agent markers and updates the selected cell when changing generations', () => {
    saved.request.initial_world.cells = [
      [
        { type_id: 0, properties: {} },
        { type_id: 0, properties: {} },
      ],
    ];
    saved.history = [0, 1].map((generation) => ({
      generation,
      world: {
        cells: [
          [
            { type_id: 0, properties: {} },
            { type_id: 0, properties: {} },
          ],
        ],
        agents: [
          { id: 5, position: { row: 0, col: generation }, state: { type_id: 0, properties: {} } },
        ],
      },
    }));
    http.expectOne('/api/simulations/7').flush(saved);
    fixture.detectChanges();
    const first = fixture.nativeElement.querySelector(
      'button[aria-label="Celula 0, 0: Gol"]',
    ) as HTMLButtonElement;
    expect(first.querySelector('span')).toBeTruthy();
    first.click();
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('aside').textContent).toContain('Agent #5');
    component.selectFrame(1);
    fixture.detectChanges();
    expect(first.querySelector('span')).toBeNull();
    expect(component.inspectedAgents()).toEqual([]);
    const second = fixture.nativeElement.querySelector(
      'button[aria-label="Celula 0, 1: Gol"]',
    ) as HTMLButtonElement;
    expect(second.querySelector('span')).toBeTruthy();
    second.click();
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('aside').textContent).toContain('Agent #5');
    expect(saved.history[0].world.agents[0].position.col).toBe(0);
  });

  it('reuses the in-flight preparation when the user reaches the end and keeps cached navigation immediate', () => {
    http.expectOne('/api/simulations/7').flush(saved);
    const step = http.expectOne('/api/simulations/7/step');
    component.next();
    expect(component.generation()?.generation).toBe(1);
    http.expectNone('/api/simulations/7/step');
    component.next();
    component.next();
    http.expectNone('/api/simulations/7/step');
    expect(step.request.method).toBe('POST');
    expect(step.request.body).toEqual({ generation: 1 });
    step.flush({ generation: 2, world: saved.request.initial_world });
    const refill = http.expectOne('/api/simulations/7/step');
    expect(refill.request.body).toEqual({ generation: 2 });
    expect(component.generation()?.generation).toBe(2);
    expect(component.simulation()?.request.generations).toBe(2);
    expect(component.simulation()?.history.map((frame) => frame.generation)).toEqual([0, 1, 2]);
    fixture.detectChanges();
    const selector: HTMLSelectElement = fixture.nativeElement.querySelector(
      '[aria-label="Generația afișată"]',
    );
    expect(selector.value).toBe('2');
    component.selectFrame(1);
    fixture.detectChanges();
    expect(selector.value).toBe('1');
    component.next();
    http.expectNone('/api/simulations/7/step');
    expect(component.generation()?.generation).toBe(2);
  });

  it('keeps the displayed world after a failed step and permits a retry', () => {
    http.expectOne('/api/simulations/7').flush(saved);
    component.selectFrame(1);
    component.next();
    http
      .expectOne('/api/simulations/7/step')
      .flush({ error: 'Invalid generated value' }, { status: 422, statusText: 'Invalid' });
    fixture.detectChanges();
    expect(component.generation()?.generation).toBe(1);
    expect(component.simulation()?.history).toEqual(saved.history);
    expect(fixture.nativeElement.querySelector('h1').textContent).toBe('Test');
    expect(fixture.nativeElement.querySelector('[role="alert"]').textContent).toContain(
      'Invalid generated value',
    );
    expect(component.advancing()).toBe(false);
    component.next();
    http
      .expectOne('/api/simulations/7/step')
      .flush({ generation: 2, world: saved.request.initial_world });
    expect(component.stepError()).toBe('');
  });

  it('cancels an in-flight step on navigation without replacing the new simulation', () => {
    http.expectOne('/api/simulations/7').flush(saved);
    component.selectFrame(1);
    component.next();
    const obsolete = http.expectOne('/api/simulations/7/step');
    params.next(convertToParamMap({ id: '8' }));
    expect(obsolete.cancelled).toBe(true);
    http.expectOne('/api/simulations/8').flush({ ...saved, id: 8 });
    expect(component.simulation()?.id).toBe(8);
    expect(component.advancing()).toBe(false);
  });

  it('continues beyond generation 1000 and respects older final-only simulations', () => {
    saved.request.save_history = false;
    saved.request.generations = 1000;
    saved.history = [{ generation: 1000, world: saved.request.initial_world }];
    http.expectOne('/api/simulations/7').flush(saved);
    component.next();
    http
      .expectOne('/api/simulations/7/step')
      .flush({ generation: 1001, world: saved.request.initial_world });
    expect(component.generation()?.generation).toBe(1001);
    expect(component.simulation()?.history.length).toBe(1);
    fixture.detectChanges();
    const next: HTMLButtonElement = fixture.nativeElement.querySelector(
      '[aria-label="Generația următoare"]',
    );
    expect(next.disabled).toBe(false);
  });

  it('prepares only three generations ahead without advancing the visible world', () => {
    saved.history = [saved.history[0]];
    saved.request.generations = 0;
    http.expectOne('/api/simulations/7').flush(saved);
    for (let generation = 0; generation < 3; generation++) {
      const preparation = http.expectOne('/api/simulations/7/step');
      expect(preparation.request.body).toEqual({ generation });
      expect(component.prefetching()).toBe(true);
      preparation.flush({ generation: generation + 1, world: saved.request.initial_world });
      expect(component.generation()?.generation).toBe(0);
    }
    http.expectNone('/api/simulations/7/step');
    expect(component.prefetching()).toBe(false);
    expect(component.simulation()?.history.map((frame) => frame.generation)).toEqual([0, 1, 2, 3]);
    component.next();
    expect(component.generation()?.generation).toBe(1);
    expect(component.advancing()).toBe(false);
    const refill = http.expectOne('/api/simulations/7/step');
    expect(refill.request.body).toEqual({ generation: 3 });
    component.selectFrame(0);
    refill.flush({ generation: 4, world: saved.request.initial_world });
    expect(component.generation()?.generation).toBe(0);
    http.expectNone('/api/simulations/7/step');
  });

  it('pauses failed preparation without disturbing the world and retries only on an explicit step', () => {
    http.expectOne('/api/simulations/7').flush(saved);
    http
      .expectOne('/api/simulations/7/step')
      .flush({ error: 'Temporary error' }, { status: 500, statusText: 'Error' });
    expect(component.generation()?.generation).toBe(0);
    expect(component.stepError()).toBe('');
    component.next();
    expect(component.generation()?.generation).toBe(1);
    http.expectNone('/api/simulations/7/step');
    component.next();
    expect(component.advancing()).toBe(true);
    http
      .expectOne('/api/simulations/7/step')
      .flush({ generation: 2, world: saved.request.initial_world });
    expect(component.generation()?.generation).toBe(2);
  });

  it('keeps the same navigation status element while calculation starts and stops', () => {
    saved.history = [saved.history[0]];
    http.expectOne('/api/simulations/7').flush(saved);
    fixture.detectChanges();
    const status: HTMLElement = fixture.nativeElement.querySelector('nav [role="status"]');
    const world: HTMLElement = fixture.nativeElement.querySelector(
      '[aria-label="Grila simulării"]',
    );
    expect(status.getAttribute('aria-busy')).toBe('true');
    http
      .expectOne('/api/simulations/7/step')
      .flush({ error: 'Temporary error' }, { status: 500, statusText: 'Error' });
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('nav [role="status"]')).toBe(status);
    expect(status.getAttribute('aria-busy')).toBe('false');
    expect(fixture.nativeElement.querySelector('[aria-label="Grila simulării"]')).toBe(world);
    expect(fixture.nativeElement.querySelector('nav').nextElementSibling.textContent).not.toContain(
      'Se calculează',
    );
  });

  it('pauses preparation during confirmation and resumes after cancellation', () => {
    http.expectOne('/api/simulations/7').flush(saved);
    const preparation = http.expectOne('/api/simulations/7/step');
    component.askDelete();
    component.next();
    preparation.flush({ generation: 2, world: saved.request.initial_world });
    expect(component.generation()?.generation).toBe(0);
    http.expectNone('/api/simulations/7/step');
    http.expectNone((request) => request.method === 'DELETE');
    component.cancelDelete();
    expect(component.confirmingDelete()).toBe(false);
    http.expectOne('/api/simulations/7/step');
  });

  it('deletes only after confirmation, prevents duplicate requests and opens the list', () => {
    http.expectOne('/api/simulations/7').flush(saved);
    const preparation = http.expectOne('/api/simulations/7/step');
    const navigate = vi.spyOn(TestBed.inject(Router), 'navigate').mockResolvedValue(true);
    component.confirmDelete();
    http.expectNone((request) => request.method === 'DELETE');
    component.askDelete();
    component.confirmDelete();
    component.confirmDelete();
    const deletion = http.expectOne((request) => request.method === 'DELETE');
    expect(deletion.request.url).toBe('/api/simulations/7');
    deletion.flush(null, { status: 204, statusText: 'No Content' });
    expect(navigate).toHaveBeenCalledWith(['/simulations']);
    expect(preparation.cancelled).toBe(true);
  });

  it('keeps the simulation and confirmation after a deletion error and permits retry', () => {
    vi.spyOn(TestBed.inject(Router), 'navigate').mockResolvedValue(true);
    http.expectOne('/api/simulations/7').flush(saved);
    component.askDelete();
    component.confirmDelete();
    http
      .expectOne((request) => request.method === 'DELETE')
      .flush({ error: 'Storage unavailable' }, { status: 500, statusText: 'Error' });
    fixture.detectChanges();
    expect(component.simulation()?.id).toBe(7);
    expect(component.confirmingDelete()).toBe(true);
    expect(component.deleting()).toBe(false);
    expect(fixture.nativeElement.querySelector('[role="alert"]').textContent).toContain(
      'Storage unavailable',
    );
    component.confirmDelete();
    http
      .expectOne((request) => request.method === 'DELETE')
      .flush(null, { status: 204, statusText: 'No Content' });
  });
});
