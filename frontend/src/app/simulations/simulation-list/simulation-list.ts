import { DatePipe } from '@angular/common';
import { Component, DestroyRef, inject, signal } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { FormControl, FormGroup, ReactiveFormsModule } from '@angular/forms';
import { ActivatedRoute, ParamMap, Router, RouterLink } from '@angular/router';
import {
  catchError,
  combineLatest,
  debounceTime,
  finalize,
  of,
  startWith,
  Subject,
  switchMap,
} from 'rxjs';
import { SimulationsApi, simulationErrorMessage } from '../simulations-api';
import {
  SimulationListQuery,
  SimulationListResponse,
  SimulationSort,
  SimulationSummary,
  SortOrder,
} from '../simulation.models';

@Component({
  imports: [DatePipe, ReactiveFormsModule, RouterLink],
  selector: 'app-simulation-list',
  styleUrl: './simulation-list.css',
  templateUrl: './simulation-list.html',
})
export class SimulationList {
  private readonly api = inject(SimulationsApi);
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);
  private readonly destroyRef = inject(DestroyRef);
  private readonly refresh = new Subject<void>();

  readonly loading = signal(true);
  readonly error = signal('');
  readonly pendingDelete = signal<SimulationSummary | null>(null);
  readonly deleting = signal(false);
  readonly deleteError = signal('');
  readonly result = signal<SimulationListResponse | null>(null);
  readonly query = signal<SimulationListQuery>({
    page: 1,
    page_size: 20,
    search: '',
    sort: 'created_at',
    order: 'desc',
  });
  readonly filters = new FormGroup({
    search: new FormControl('', { nonNullable: true }),
    sort: new FormControl<SimulationSort>('created_at', { nonNullable: true }),
    order: new FormControl<SortOrder>('desc', { nonNullable: true }),
    page_size: new FormControl(20, { nonNullable: true }),
  });
  readonly orderings: { label: string; sort: SimulationSort; order: SortOrder }[] = [
    { label: 'Cele mai recente', sort: 'created_at', order: 'desc' },
    { label: 'Cele mai vechi', sort: 'created_at', order: 'asc' },
    { label: 'Nume, A–Z', sort: 'name', order: 'asc' },
    { label: 'Nume, Z–A', sort: 'name', order: 'desc' },
    { label: 'Generații, crescător', sort: 'generations', order: 'asc' },
    { label: 'Generații, descrescător', sort: 'generations', order: 'desc' },
  ];

  constructor() {
    combineLatest([this.route.queryParamMap, this.refresh.pipe(startWith(undefined))])
      .pipe(
        switchMap(([params]) => {
          const query = this.readQuery(params);
          this.query.set(query);
          this.filters.setValue(
            {
              search: query.search,
              sort: query.sort,
              order: query.order,
              page_size: query.page_size,
            },
            { emitEvent: false },
          );
          this.loading.set(true);
          this.pendingDelete.set(null);
          this.deleteError.set('');
          this.error.set('');
          return this.api.list(query).pipe(
            catchError((error: unknown) => {
              this.error.set(simulationErrorMessage(error));
              return of(null);
            }),
          );
        }),
        takeUntilDestroyed(),
      )
      .subscribe((result) => {
        this.result.set(result);
        this.loading.set(false);
      });
    this.filters.controls.search.valueChanges
      .pipe(debounceTime(250), takeUntilDestroyed())
      .subscribe((search) => {
        if (search.trim() !== this.query().search) this.applyFilters();
      });
  }

  changeOrdering(event: Event) {
    const value = (event.target as HTMLSelectElement).value;
    const ordering = this.orderings.find((item) => item.sort + ':' + item.order === value);
    if (!ordering) return;
    this.filters.patchValue({ sort: ordering.sort, order: ordering.order });
    this.applyFilters();
  }

  applyFilters() {
    const filters = this.filters.getRawValue();
    const query = { ...filters, search: filters.search.trim(), page: 1 };
    const current = this.query();
    if (
      current.page === 1 &&
      query.search === current.search &&
      query.sort === current.sort &&
      query.order === current.order &&
      query.page_size === current.page_size
    ) {
      this.reload();
      return;
    }
    void this.router.navigate([], { relativeTo: this.route, queryParams: query });
  }

  changePage(page: number) {
    void this.router.navigate([], {
      relativeTo: this.route,
      queryParams: { ...this.query(), page },
    });
  }

  reload() {
    this.refresh.next();
  }

  askDelete(simulation: SimulationSummary) {
    if (this.deleting()) return;
    this.pendingDelete.set(simulation);
    this.deleteError.set('');
  }

  cancelDelete() {
    if (this.deleting()) return;
    this.pendingDelete.set(null);
    this.deleteError.set('');
  }

  confirmDelete() {
    const simulation = this.pendingDelete();
    if (!simulation || this.deleting()) return;
    this.deleting.set(true);
    this.deleteError.set('');
    this.api
      .delete(simulation.id)
      .pipe(
        takeUntilDestroyed(this.destroyRef),
        finalize(() => this.deleting.set(false)),
      )
      .subscribe({
        next: () => {
          const current = this.result();
          this.pendingDelete.set(null);
          if (
            current?.items.length === 1 &&
            current.items[0].id === simulation.id &&
            this.query().page > 1
          ) {
            this.changePage(this.query().page - 1);
          } else {
            this.reload();
          }
        },
        error: (error: unknown) => this.deleteError.set(simulationErrorMessage(error)),
      });
  }

  private readQuery(params: ParamMap): SimulationListQuery {
    const page = Number(params.get('page') ?? 1);
    const pageSize = Number(params.get('page_size') ?? 20);
    const sort = params.get('sort');
    const order = params.get('order');
    return {
      page: Number.isInteger(page) && page > 0 && page <= 4294967295 ? page : 1,
      page_size: Number.isInteger(pageSize) && pageSize >= 1 && pageSize <= 100 ? pageSize : 20,
      search: params.get('search') ?? '',
      sort: sort === 'name' || sort === 'generations' ? sort : 'created_at',
      order: order === 'asc' ? 'asc' : 'desc',
    };
  }
}
