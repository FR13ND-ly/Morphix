import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { ActivatedRoute, convertToParamMap, provideRouter, Router } from '@angular/router';
import { BehaviorSubject } from 'rxjs';
import { vi } from 'vitest';
import { SimulationList } from './simulation-list';
import { SimulationSummary } from '../simulation.models';

describe('SimulationList', () => {
  let component: SimulationList;
  let fixture: ComponentFixture<SimulationList>;
  let http: HttpTestingController;
  let params: BehaviorSubject<ReturnType<typeof convertToParamMap>>;

  beforeEach(async () => {
    params = new BehaviorSubject(convertToParamMap({}));
    await TestBed.configureTestingModule({
      imports: [SimulationList],
      providers: [
        provideHttpClient(),
        provideHttpClientTesting(),
        provideRouter([]),
        { provide: ActivatedRoute, useValue: { queryParamMap: params.asObservable() } },
      ],
    }).compileComponents();

    fixture = TestBed.createComponent(SimulationList);
    component = fixture.componentInstance;
    http = TestBed.inject(HttpTestingController);
  });

  afterEach(() => http.verify());

  function flushList(items: object[] = [], total = items.length) {
    http
      .expectOne((req) => req.url === '/api/simulations')
      .flush({ items, total, page: component.query().page, page_size: 20 });
    fixture.detectChanges();
  }

  it('shows the empty state after loading an empty database', () => {
    flushList();
    expect(fixture.nativeElement.textContent).toContain('Nu există simulări');
    expect(component.loading()).toBe(false);
  });

  it('renders saved metadata with a link to the result', () => {
    flushList([
      {
        id: 7,
        name: 'Diffusion',
        created_at: '2026-11-04T00:00:00Z',
        width: 5,
        height: 3,
        generations: 12,
        save_history: false,
      },
    ]);
    expect(fixture.nativeElement.textContent).toContain('Diffusion');
    expect(fixture.nativeElement.querySelector('a[href="/simulations/7"]')).toBeTruthy();
  });

  it('resets the page when applying filters and keeps them when paging', () => {
    flushList();
    params.next(convertToParamMap({ page: '3', search: 'Life', sort: 'name', order: 'asc' }));
    const call = http.expectOne((req) => req.params.get('page') === '3');
    expect(call.request.params.get('search')).toBe('Life');
    call.flush({ items: [], total: 100, page: 3, page_size: 20 });
    const navigate = vi.spyOn(TestBed.inject(Router), 'navigate').mockResolvedValue(true);
    component.filters.controls.search.setValue(' Diffusion ');
    component.applyFilters();
    expect(navigate).toHaveBeenLastCalledWith(
      [],
      expect.objectContaining({
        queryParams: { page: 1, page_size: 20, search: 'Diffusion', sort: 'name', order: 'asc' },
      }),
    );
    component.changePage(4);
    expect(navigate).toHaveBeenLastCalledWith(
      [],
      expect.objectContaining({
        queryParams: { page: 4, page_size: 20, search: 'Life', sort: 'name', order: 'asc' },
      }),
    );
  });

  it('shows an API error and can retry', () => {
    http
      .expectOne((req) => req.url === '/api/simulations')
      .flush({ error: 'Storage unavailable' }, { status: 500, statusText: 'Error' });
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('[role="alert"]').textContent).toContain(
      'Storage unavailable',
    );
    component.reload();
    flushList();
    expect(component.error()).toBe('');
  });

  it('applies the selected sorting direction through the single order control', () => {
    flushList();
    const navigate = vi.spyOn(TestBed.inject(Router), 'navigate').mockResolvedValue(true);
    const select = fixture.nativeElement.querySelector('form select') as HTMLSelectElement;
    select.value = 'generations:desc';
    select.dispatchEvent(new Event('change'));
    expect(navigate).toHaveBeenCalledWith(
      [],
      expect.objectContaining({
        queryParams: { page: 1, page_size: 20, search: '', sort: 'generations', order: 'desc' },
      }),
    );
  });

  it('waits for a pause in typing before applying search', () => {
    flushList();
    vi.useFakeTimers();
    try {
      const navigate = vi.spyOn(TestBed.inject(Router), 'navigate').mockResolvedValue(true);
      component.filters.controls.search.setValue('Life');
      vi.advanceTimersByTime(249);
      expect(navigate).not.toHaveBeenCalled();
      vi.advanceTimersByTime(1);
      expect(navigate).toHaveBeenCalledWith(
        [],
        expect.objectContaining({
          queryParams: expect.objectContaining({ page: 1, search: 'Life' }),
        }),
      );
    } finally {
      vi.useRealTimers();
    }
  });

  it('cancels an obsolete request when the URL filters change', () => {
    const obsolete = http.expectOne((req) => req.url === '/api/simulations');
    params.next(convertToParamMap({ search: 'Diffusion' }));
    expect(obsolete.cancelled).toBe(true);
    flushList();
    expect(component.query().search).toBe('Diffusion');
  });

  const simulation: SimulationSummary = {
    id: 7,
    name: 'Life',
    created_at: '2026-11-04T00:00:00Z',
    width: 3,
    height: 3,
    generations: 2,
    save_history: true,
  };

  it('cancels a deletion confirmation without sending a request', () => {
    flushList([simulation]);
    component.askDelete(simulation);
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Șterge definitiv');
    component.cancelDelete();
    expect(component.pendingDelete()).toBeNull();
    http.expectNone((request) => request.method === 'DELETE');
  });

  it('deletes the selected simulation once and refreshes the current list', () => {
    flushList([simulation]);
    component.askDelete(simulation);
    component.confirmDelete();
    component.confirmDelete();
    const deletion = http.expectOne((request) => request.method === 'DELETE');
    expect(deletion.request.url).toBe('/api/simulations/7');
    deletion.flush(null, { status: 204, statusText: 'No Content' });
    flushList();
    expect(component.pendingDelete()).toBeNull();
    expect(component.result()?.total).toBe(0);
  });

  it('returns to the previous page when its last remaining row is deleted', () => {
    flushList();
    params.next(convertToParamMap({ page: '2', search: 'Life' }));
    flushList([simulation], 21);
    const navigate = vi.spyOn(TestBed.inject(Router), 'navigate').mockResolvedValue(true);
    component.askDelete(simulation);
    component.confirmDelete();
    http
      .expectOne((request) => request.method === 'DELETE')
      .flush(null, { status: 204, statusText: 'No Content' });
    expect(navigate).toHaveBeenCalledWith(
      [],
      expect.objectContaining({
        queryParams: expect.objectContaining({ page: 1, search: 'Life' }),
      }),
    );
  });

  it('preserves the row after a deletion failure and lets the user retry', () => {
    flushList([simulation]);
    component.askDelete(simulation);
    component.confirmDelete();
    http
      .expectOne((request) => request.method === 'DELETE')
      .flush({ error: 'Storage unavailable' }, { status: 500, statusText: 'Error' });
    fixture.detectChanges();
    expect(component.result()?.items[0].id).toBe(7);
    expect(component.deleting()).toBe(false);
    expect(fixture.nativeElement.querySelector('[role="alert"]').textContent).toContain(
      'Storage unavailable',
    );
    component.confirmDelete();
    http
      .expectOne((request) => request.method === 'DELETE')
      .flush(null, { status: 204, statusText: 'No Content' });
    flushList();
  });
});
