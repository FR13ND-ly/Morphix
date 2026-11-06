import { TestBed } from '@angular/core/testing';
import { HttpErrorResponse, provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { SimulationsApi, simulationErrorMessage } from './simulations-api';
import { SimulationRequest } from './simulation.models';

describe('SimulationsApi', () => {
  let service: SimulationsApi;
  let http: HttpTestingController;
  const request: SimulationRequest = {
    automaton: {
      name: 'Test',
      object_types: [{ id: 0, name: 'Gol', properties: {} }],
      neighborhood: { offsets: [[0, 1]] },
      boundary: { kind: 'wrap' },
      rules: [],
    },
    initial_world: { cells: [[{ type_id: 0, properties: {} }]], agents: [] },
    generations: 0,
    seed: 42,
    save_history: false,
  };

  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting()],
    });
    service = TestBed.inject(SimulationsApi);
    http = TestBed.inject(HttpTestingController);
  });

  afterEach(() => http.verify());

  it('sends the list filters as query parameters', () => {
    service
      .list({ page: 2, page_size: 10, search: 'viață & apă', sort: 'name', order: 'asc' })
      .subscribe();
    const call = http.expectOne((req) => req.url === '/api/simulations');
    expect(call.request.params.get('search')).toBe('viață & apă');
    expect(call.request.params.get('page')).toBe('2');
    expect(call.request.params.get('page_size')).toBe('10');
    expect(call.request.params.get('sort')).toBe('name');
    expect(call.request.params.get('order')).toBe('asc');
    call.flush({ items: [], total: 0, page: 2, page_size: 10 });
  });

  it('posts the same configuration for validation and execution', () => {
    service.validate(request).subscribe();
    const validation = http.expectOne('/api/simulations/validate');
    expect(validation.request.method).toBe('POST');
    expect(validation.request.body).toEqual(request);
    validation.flush({ valid: true });
    service.run(request).subscribe((result) => expect(result.id).toBe(7));
    const execution = http.expectOne('/api/simulations/run');
    expect(execution.request.body).toEqual(request);
    execution.flush({
      id: 7,
      seed: 42,
      history: [{ generation: 0, world: request.initial_world }],
    });
  });

  it('uses the explicit generation number when fetching a snapshot', () => {
    service.getGeneration(7, 12).subscribe((snapshot) => expect(snapshot.generation).toBe(12));
    http
      .expectOne('/api/simulations/7/generations/12')
      .flush({ generation: 12, world: request.initial_world });
  });

  it('rejects unsafe integer seeds before sending a rounded configuration', () => {
    let message = '';
    service.run({ ...request, seed: Number.MAX_SAFE_INTEGER + 1 }).subscribe({
      error: (error: unknown) => (message = simulationErrorMessage(error)),
    });
    expect(message).toContain('9007199254740991');
    http.expectNone('/api/simulations/run');
  });

  it('rejects saved configurations whose agent IDs cannot be represented exactly', () => {
    let message = '';
    service
      .get(7)
      .subscribe({ error: (error: unknown) => (message = simulationErrorMessage(error)) });
    http.expectOne('/api/simulations/7').flush({
      id: 7,
      created_at: '2026-11-04T00:00:00Z',
      history: [],
      request: {
        ...request,
        initial_world: {
          ...request.initial_world,
          agents: [
            {
              id: Number.MAX_SAFE_INTEGER + 1,
              position: { row: 0, col: 0 },
              state: { type_id: 0, properties: {} },
            },
          ],
        },
      },
    });
    expect(message).toContain('9007199254740991');
  });

  it('keeps backend validation messages and explains connection failures', () => {
    expect(
      simulationErrorMessage(
        new HttpErrorResponse({ status: 422, error: { error: 'Unknown type 9' } }),
      ),
    ).toBe('Unknown type 9');
    expect(simulationErrorMessage(new HttpErrorResponse({ status: 0 }))).toContain('Rust');
  });
});
