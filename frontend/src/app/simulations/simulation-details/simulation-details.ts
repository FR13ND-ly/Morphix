import { DatePipe, JsonPipe, KeyValuePipe } from '@angular/common';
import { Component, computed, DestroyRef, inject, signal } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { ActivatedRoute, Router } from '@angular/router';
import { catchError, finalize, of, Subscription, switchMap, throwError } from 'rxjs';
import { SimulationsApi, simulationErrorMessage } from '../simulations-api';
import { Agent, Position, StoredSimulation } from '../simulation.models';

@Component({
  imports: [DatePipe, JsonPipe, KeyValuePipe],
  selector: 'app-simulation-details',
  styleUrl: './simulation-details.css',
  templateUrl: './simulation-details.html',
})
export class SimulationDetails {
  private readonly api = inject(SimulationsApi);
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);
  private readonly destroyRef = inject(DestroyRef);
  private stepSubscription?: Subscription;
  private deleteSubscription?: Subscription;
  private readonly lookAhead = 3;
  private prefetchPaused = false;
  private stopped = false;

  readonly loading = signal(true);
  readonly error = signal('');
  readonly stepError = signal('');
  readonly advancing = signal(false);
  readonly prefetching = signal(false);
  readonly confirmingDelete = signal(false);
  readonly deleting = signal(false);
  readonly deleteError = signal('');
  readonly simulation = signal<StoredSimulation | null>(null);
  readonly selectedIndex = signal(0);
  readonly selectedCell = signal<Position | null>(null);
  readonly generation = computed(() => this.simulation()?.history[this.selectedIndex()] ?? null);
  readonly inspectedState = computed(() => {
    const position = this.selectedCell();
    return position ? (this.generation()?.world.cells[position.row]?.[position.col] ?? null) : null;
  });
  readonly agentsByCell = computed(() => {
    const positions = new Map<string, Agent[]>();
    for (const agent of this.generation()?.world.agents ?? []) {
      const key = agent.position.row + ',' + agent.position.col;
      const agents = positions.get(key) ?? [];
      agents.push(agent);
      positions.set(key, agents);
    }
    return positions;
  });
  readonly inspectedAgents = computed(() => {
    const position = this.selectedCell();
    return position ? (this.agentsByCell().get(position.row + ',' + position.col) ?? []) : [];
  });
  readonly gridWidth = computed(() => {
    const columns = this.generation()?.world.cells[0]?.length ?? 1;
    return Math.max(336, columns * 12 + columns - 1);
  });
  private readonly typeColors = computed(() => {
    const colors = [
      'var(--world-empty)',
      'var(--world-filled)',
      'var(--world-type-2)',
      'var(--world-type-3)',
      'var(--world-type-4)',
    ];
    return new Map(
      this.simulation()?.request.automaton.object_types.map((type, index) => [
        type.id,
        colors[index % colors.length],
      ]) ?? [],
    );
  });
  private readonly typeNames = computed(
    () =>
      new Map(
        this.simulation()?.request.automaton.object_types.map((type) => [type.id, type.name]) ?? [],
      ),
  );

  constructor() {
    this.destroyRef.onDestroy(() => {
      this.stopped = true;
      this.stepSubscription?.unsubscribe();
    });
    this.route.paramMap
      .pipe(
        switchMap((params) => {
          this.simulation.set(null);
          this.stepSubscription?.unsubscribe();
          this.deleteSubscription?.unsubscribe();
          this.confirmingDelete.set(false);
          this.deleteError.set('');
          this.prefetchPaused = false;
          this.advancing.set(false);
          this.stepError.set('');
          this.loading.set(true);
          this.error.set('');
          this.selectedIndex.set(0);
          this.selectedCell.set(null);
          const id = params.get('id') ?? '';
          const request = /^[1-9]\d*$/.test(id)
            ? this.api.get(id)
            : throwError(() => new Error('ID-ul simulării este invalid.'));
          return request.pipe(
            catchError((error: unknown) => {
              this.error.set(simulationErrorMessage(error));
              return of(null);
            }),
          );
        }),
        takeUntilDestroyed(),
      )
      .subscribe((simulation) => {
        this.simulation.set(simulation);
        this.loading.set(false);
        this.prepareAhead();
      });
  }

  selectFrame(index: number) {
    if (this.confirmingDelete() || this.deleting()) return;
    const length = this.simulation()?.history.length ?? 0;
    if (Number.isInteger(index) && index >= 0 && index < length) {
      this.selectedIndex.set(index);
      this.advancing.set(false);
      this.prepareAhead();
    }
  }

  next() {
    const saved = this.simulation();
    const current = this.generation();
    if (this.advancing() || this.confirmingDelete() || this.deleting() || !saved || !current)
      return;
    this.stepError.set('');
    if (this.selectedIndex() < saved.history.length - 1) {
      this.selectFrame(this.selectedIndex() + 1);
      return;
    }
    this.advancing.set(true);
    this.prefetchPaused = false;
    this.fetchNext();
  }

  private prepareAhead() {
    const saved = this.simulation();
    if (
      !saved ||
      saved.request.save_history === false ||
      this.prefetchPaused ||
      this.confirmingDelete() ||
      this.deleting()
    )
      return;
    if (saved.history.length - 1 - this.selectedIndex() < this.lookAhead) {
      this.fetchNext();
    }
  }

  private fetchNext() {
    const saved = this.simulation();
    const last = saved?.history.at(-1);
    if (
      this.stopped ||
      this.prefetching() ||
      this.confirmingDelete() ||
      this.deleting() ||
      !saved ||
      !last
    )
      return;
    this.prefetching.set(true);
    let succeeded = false;
    this.stepSubscription = this.api
      .step(saved.id, last.generation)
      .pipe(
        takeUntilDestroyed(this.destroyRef),
        finalize(() => {
          this.prefetching.set(false);
          if (succeeded && !this.stopped && this.simulation()?.id === saved.id) this.prepareAhead();
        }),
      )
      .subscribe({
        next: (snapshot) => {
          const latest = this.simulation();
          if (!latest || latest.id !== saved.id) return;
          const history =
            latest.request.save_history === false ? [snapshot] : [...latest.history, snapshot];
          this.simulation.set({
            ...latest,
            request: { ...latest.request, generations: snapshot.generation },
            history,
          });
          if (this.advancing()) this.selectedIndex.set(history.length - 1);
          this.advancing.set(false);
          succeeded = true;
        },
        error: (error: unknown) => {
          this.prefetchPaused = true;
          if (this.advancing()) this.stepError.set(simulationErrorMessage(error));
          this.advancing.set(false);
        },
      });
  }

  changeFrame(event: Event) {
    this.selectFrame(Number((event.target as HTMLSelectElement).value));
  }

  askDelete() {
    if (this.deleting() || !this.simulation()) return;
    this.confirmingDelete.set(true);
    this.deleteError.set('');
    this.prefetchPaused = true;
    this.advancing.set(false);
  }

  cancelDelete() {
    if (this.deleting()) return;
    this.confirmingDelete.set(false);
    this.deleteError.set('');
    this.prefetchPaused = false;
    this.prepareAhead();
  }

  confirmDelete() {
    const saved = this.simulation();
    if (!saved || !this.confirmingDelete() || this.deleting()) return;
    this.deleting.set(true);
    this.deleteError.set('');
    this.deleteSubscription = this.api
      .delete(saved.id)
      .pipe(
        takeUntilDestroyed(this.destroyRef),
        finalize(() => this.deleting.set(false)),
      )
      .subscribe({
        next: () => {
          this.stopped = true;
          this.stepSubscription?.unsubscribe();
          void this.router.navigate(['/simulations']);
        },
        error: (error: unknown) => this.deleteError.set(simulationErrorMessage(error)),
      });
  }

  typeName(id: number) {
    return this.typeNames().get(id) ?? `Tip ${id}`;
  }

  typeColor(id: number) {
    return this.typeColors().get(id) ?? 'var(--world-empty)';
  }
}
