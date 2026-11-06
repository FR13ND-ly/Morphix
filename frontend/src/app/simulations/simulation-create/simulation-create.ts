import { Component, computed, DestroyRef, inject, signal } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { FormControl, FormGroup, ReactiveFormsModule, Validators } from '@angular/forms';
import { Router } from '@angular/router';
import { finalize, Observable, Subscription } from 'rxjs';
import { SimulationsApi, simulationErrorMessage } from '../simulations-api';
import { InitialTypeSettings, SimulationRequest } from '../simulation.models';
import { WorldInitializer } from '../world-initializer';

@Component({
  imports: [ReactiveFormsModule],
  selector: 'app-simulation-create',
  styleUrl: './simulation-create.css',
  templateUrl: './simulation-create.html',
})
export class SimulationCreate {
  private readonly api = inject(SimulationsApi);
  private readonly initializer = inject(WorldInitializer);
  private readonly router = inject(Router);
  private readonly destroyRef = inject(DestroyRef);
  private requestSubscription?: Subscription;

  readonly busy = signal<'loading' | 'validating' | 'running' | null>(null);
  readonly error = signal('');
  readonly message = signal('');
  readonly validated = signal(false);
  readonly configuration = signal<SimulationRequest | null>(null);
  readonly initialTypes = signal<InitialTypeSettings[]>([]);
  readonly source = signal('');
  readonly editingGrid = signal(false);
  readonly maxSeed = Number.MAX_SAFE_INTEGER;
  readonly parameters = new FormGroup({
    randomize: new FormControl(true, { nonNullable: true }),
    seed: new FormControl(0, {
      nonNullable: true,
      validators: [
        Validators.required,
        Validators.min(0),
        Validators.max(this.maxSeed),
        Validators.pattern(/^\d+$/),
      ],
    }),
  });
  readonly gridSize = new FormGroup({
    rows: new FormControl(1, {
      nonNullable: true,
      validators: [
        Validators.required,
        Validators.min(1),
        Validators.max(100000),
        Validators.pattern(/^\d+$/),
      ],
    }),
    columns: new FormControl(1, {
      nonNullable: true,
      validators: [
        Validators.required,
        Validators.min(1),
        Validators.max(100000),
        Validators.pattern(/^\d+$/),
      ],
    }),
  });
  readonly newCellType = computed(() => {
    const request = this.configuration();
    const typeId = request?.initial_world.cells[0][0].type_id;
    return request?.automaton.object_types.find((type) => type.id === typeId);
  });
  readonly example = new FormControl('game-of-life', { nonNullable: true });
  readonly examples = [
    { id: 'game-of-life', name: 'Game of Life' },
    { id: 'forest-fire', name: 'Incendiu de pădure' },
    { id: 'diffusion', name: 'Difuzie' },
    { id: 'artificial-ecosystem', name: 'Ecosistem cu agenți' },
  ];

  constructor() {
    this.destroyRef.onDestroy(() => this.requestSubscription?.unsubscribe());
    this.parameters.valueChanges.pipe(takeUntilDestroyed()).subscribe(() => this.clearValidation());
  }

  loadExample() {
    if (this.busy()) return;
    const selected = this.examples.find((example) => example.id === this.example.value);
    this.loadConfiguration(
      this.api.getExample(this.example.value),
      selected?.name ?? this.example.value,
    );
  }

  validate() {
    if (this.busy() || this.editingGrid()) return;
    const request = this.prepareRequest();
    if (!request) return;
    this.error.set('');
    this.message.set('');
    this.validated.set(false);
    this.busy.set('validating');
    this.requestSubscription = this.api
      .validate(request)
      .pipe(
        takeUntilDestroyed(this.destroyRef),
        finalize(() => this.busy.set(null)),
      )
      .subscribe({
        next: (response) => {
          this.validated.set(response.valid);
          this.message.set(
            response.valid ? 'Configurația este validă.' : 'Configurația nu este validă.',
          );
        },
        error: (error: unknown) => this.error.set(simulationErrorMessage(error)),
      });
  }

  run() {
    if (this.busy() || this.editingGrid()) return;
    const request = this.prepareRequest();
    if (!request) return;
    this.error.set('');
    this.message.set('');
    this.busy.set('running');
    this.requestSubscription = this.api
      .run(request)
      .pipe(
        takeUntilDestroyed(this.destroyRef),
        finalize(() => this.busy.set(null)),
      )
      .subscribe({
        next: (response) => void this.router.navigate(['/simulations', response.id]),
        error: (error: unknown) => this.error.set(simulationErrorMessage(error)),
      });
  }

  generateSeed() {
    if (this.busy() || !this.configuration()) return;
    const seed = crypto.getRandomValues(new Uint32Array(1))[0];
    this.parameters.controls.seed.setValue(seed);
    this.parameters.controls.seed.markAsDirty();
  }

  changeTypeSetting(
    typeId: number,
    field: 'cell_weight' | 'agent_count' | 'agent_cell_type',
    event: Event,
  ) {
    if (this.busy()) return;
    const input = event.target as HTMLInputElement;
    const value = input.value === '' ? null : Number(input.value);
    this.initialTypes.update((types) =>
      types.map((type) => (type.type_id === typeId ? { ...type, [field]: value } : type)),
    );
    this.clearValidation();
  }

  changePropertySetting(typeId: number, name: string, field: 'minimum' | 'maximum', event: Event) {
    if (this.busy()) return;
    const input = event.target as HTMLInputElement;
    const value = input.value === '' ? null : Number(input.value);
    this.initialTypes.update((types) =>
      types.map((type) =>
        type.type_id === typeId
          ? {
              ...type,
              properties: type.properties.map((property) =>
                property.name === name ? { ...property, [field]: value } : property,
              ),
            }
          : type,
      ),
    );
    this.clearValidation();
  }

  editGridSize() {
    const request = this.configuration();
    if (this.busy() || !request) return;
    this.gridSize.reset({
      rows: request.initial_world.cells.length,
      columns: request.initial_world.cells[0].length,
    });
    this.error.set('');
    this.message.set('');
    this.editingGrid.set(true);
  }

  cancelGridSize() {
    if (this.busy()) return;
    this.editingGrid.set(false);
    this.error.set('');
  }

  applyGridSize() {
    const request = this.configuration();
    const type = this.newCellType();
    if (this.busy() || !request || !type || !this.editingGrid()) return;
    this.error.set('');
    if (this.gridSize.invalid) {
      this.gridSize.markAllAsTouched();
      this.error.set('Rândurile și coloanele trebuie să fie numere întregi pozitive.');
      return;
    }
    if (!this.validParameters()) return;
    const { rows, columns } = this.gridSize.getRawValue();
    if (
      request.initial_world.agents.some(
        (agent) => agent.position.row >= rows || agent.position.col >= columns,
      )
    ) {
      this.error.set(
        'Dimensiunea aleasă ar elimina agenți. Alege o grilă care să cuprindă pozițiile lor inițiale.',
      );
      return;
    }
    if (!this.fitsHistoryBudget(rows, columns)) return;

    const cells = Array.from({ length: rows }, (_, row) =>
      Array.from(
        { length: columns },
        (_, col) =>
          request.initial_world.cells[row]?.[col] ?? {
            type_id: type.id,
            properties: Object.fromEntries(
              Object.entries(type.properties).map(([name, property]) => [
                name,
                property.default_value,
              ]),
            ),
          },
      ),
    );
    this.configuration.set({
      ...request,
      initial_world: { ...request.initial_world, cells },
    });
    this.editingGrid.set(false);
    this.clearValidation();
    this.message.set(`Grila a fost schimbată la ${rows} × ${columns}.`);
  }

  private validParameters() {
    if (this.parameters.valid) return true;
    this.parameters.markAllAsTouched();
    this.error.set('Seed-ul trebuie să fie un număr întreg în intervalul indicat.');
    return false;
  }

  private prepareRequest(): SimulationRequest | null {
    const configuration = this.configuration();
    if (!configuration || !this.validParameters()) return null;
    if (
      !this.fitsHistoryBudget(
        configuration.initial_world.cells.length,
        configuration.initial_world.cells[0].length,
      )
    )
      return null;
    const { seed, randomize } = this.parameters.getRawValue();
    try {
      return {
        ...configuration,
        seed,
        initial_world: randomize
          ? this.initializer.generate(configuration, this.initialTypes(), seed)
          : configuration.initial_world,
        generations: 0,
        save_history: true,
      };
    } catch (error: unknown) {
      this.error.set(simulationErrorMessage(error));
      return null;
    }
  }

  private fitsHistoryBudget(rows: number, columns: number) {
    const request = this.configuration();
    if (!request) return false;
    const maxProperties = Math.max(
      0,
      ...request.automaton.object_types.map((type) => Object.keys(type.properties).length),
    );
    // Limit one world before allocating it; history is generated one step at a time.
    const agentCount = this.parameters.controls.randomize.value
      ? this.initialTypes().reduce((total, type) => total + Math.max(0, type.agent_count ?? 0), 0)
      : request.initial_world.agents.length;
    const units = (rows * columns + agentCount) * (maxProperties + 1);
    if (units <= 100000) return true;
    this.error.set(
      'Grila depășește limita de dimensiune a simulatorului. Micșorează numărul de celule.',
    );
    return false;
  }

  private clearValidation() {
    this.validated.set(false);
    this.error.set('');
    this.message.set('');
  }

  private loadConfiguration(request: Observable<SimulationRequest>, source: string) {
    this.clearConfiguration();
    this.source.set(source);
    this.busy.set('loading');
    this.requestSubscription = request
      .pipe(
        takeUntilDestroyed(this.destroyRef),
        finalize(() => this.busy.set(null)),
      )
      .subscribe({
        next: (configuration) => {
          this.parameters.reset(
            {
              seed: configuration.seed,
              randomize: true,
            },
            { emitEvent: false },
          );
          this.configuration.set(configuration);
          this.initialTypes.set(this.initializer.settings(configuration));
          this.message.set('Configurația a fost încărcată.');
        },
        error: (error: unknown) => this.error.set(simulationErrorMessage(error)),
      });
  }

  private clearConfiguration() {
    this.requestSubscription?.unsubscribe();
    this.configuration.set(null);
    this.initialTypes.set([]);
    this.editingGrid.set(false);
    this.source.set('');
    this.validated.set(false);
    this.error.set('');
    this.message.set('');
  }
}
