import { Component, DestroyRef, OnInit, computed, inject, signal } from '@angular/core';
import { FormControl, FormGroup, ReactiveFormsModule, Validators } from '@angular/forms';
import { HttpErrorResponse } from '@angular/common/http';
import { SimulationRequest, SimulationResponse, Position } from '../simulation.models';
import { SimulationService } from '../simulation.service';
import { WorldGrid, typeColor } from '../world-grid/world-grid';
import { EXAMPLES } from './examples';

@Component({
  selector: 'app-simulator',
  imports: [ReactiveFormsModule, WorldGrid],
  templateUrl: './simulator.html',
  styleUrl: './simulator.css',
})
export class Simulator implements OnInit {
  private readonly service = inject(SimulationService);
  private timer?: ReturnType<typeof setInterval>;
  readonly examples = EXAMPLES;
  readonly selectedExample = signal(EXAMPLES[0]);
  readonly request = signal<SimulationRequest | null>(null);
  readonly result = signal<SimulationResponse | null>(null);
  readonly loading = signal(false);
  readonly running = signal(false);
  readonly connected = signal<boolean | null>(null);
  readonly error = signal('');
  readonly progress = signal('');
  readonly frameIndex = signal(0);
  readonly playing = signal(false);
  readonly selectedCell = signal<Position | null>(null);

  readonly settings = new FormGroup({
    generations: new FormControl(20, {
      nonNullable: true,
      validators: [
        Validators.required,
        Validators.min(0),
        Validators.max(1000),
        Validators.pattern(/^\d+$/),
      ],
    }),
    seed: new FormControl(42, {
      nonNullable: true,
      validators: [
        Validators.required,
        Validators.min(0),
        Validators.max(Number.MAX_SAFE_INTEGER),
        Validators.pattern(/^\d+$/),
      ],
    }),
    saveHistory: new FormControl(true, { nonNullable: true }),
  });

  readonly frame = computed(() => this.result()?.history[this.frameIndex()] ?? null);
  readonly world = computed(() => this.frame()?.world ?? this.request()?.initial_world ?? null);
  readonly lastFrame = computed(() => (this.result()?.history.length ?? 1) - 1);
  readonly generation = computed(() => this.frame()?.generation ?? 0);
  readonly historyLabel = computed(() => {
    const count = this.result()?.history.length ?? 0;
    if (count === 0) return 'Rulează pentru a vedea evoluția';
    return count === 1 ? '1 stare disponibilă' : `${count} stări disponibile`;
  });
  readonly cellCount = computed(
    () => this.world()?.cells.reduce((sum, row) => sum + row.length, 0) ?? 0,
  );
  readonly legend = computed(() => {
    const world = this.world();
    if (!world) return [];
    return (this.request()?.automaton.object_types ?? []).map((type) => ({
      ...type,
      color: typeColor(type.id),
      cells: world.cells.flat().filter((cell) => cell.type_id === type.id).length,
      agents: world.agents.filter((agent) => agent.state.type_id === type.id).length,
    }));
  });
  readonly inspection = computed(() => {
    const position = this.selectedCell();
    const world = this.world();
    if (!position || !world) return null;
    const cell = world.cells[position.row]?.[position.col];
    if (!cell) return null;
    const types = this.request()?.automaton.object_types ?? [];
    return {
      position,
      name: types.find((type) => type.id === cell.type_id)?.name ?? String(cell.type_id),
      properties: Object.entries(cell.properties),
      agents: world.agents.filter(
        (agent) => agent.position.row === position.row && agent.position.col === position.col,
      ),
    };
  });

  constructor() {
    inject(DestroyRef).onDestroy(() => this.pause());
  }

  ngOnInit() {
    void this.loadExample(EXAMPLES[0].file);
    void this.checkConnection();
  }

  async checkConnection() {
    try {
      await this.service.health();
      this.connected.set(true);
    } catch {
      this.connected.set(false);
    }
  }

  async loadExample(file: string) {
    const example = EXAMPLES.find((item) => item.file === file);
    if (!example || this.loading() || this.running()) return;
    this.pause();
    this.loading.set(true);
    this.selectedExample.set(example);
    this.error.set('');
    this.result.set(null);
    this.request.set(null);
    this.frameIndex.set(0);
    this.selectedCell.set(null);
    try {
      const request = await this.service.loadExample(file);
      this.request.set(request);
      this.settings.setValue({
        generations: request.generations,
        seed: request.seed,
        saveHistory: true,
      });
    } catch {
      this.error.set('Exemplul nu a putut fi încărcat. Încearcă din nou.');
    } finally {
      this.loading.set(false);
    }
  }

  onExampleChange(event: Event) {
    void this.loadExample((event.target as HTMLSelectElement).value);
  }

  async run() {
    const source = this.request();
    this.settings.markAllAsTouched();
    if (!source || this.settings.invalid || this.running() || this.loading()) return;
    this.pause();
    this.error.set('');
    this.running.set(true);
    const values = this.settings.getRawValue();
    const request: SimulationRequest = {
      ...source,
      generations: Number(values.generations),
      seed: Number(values.seed),
      save_history: values.saveHistory,
    };
    try {
      this.progress.set('Verificăm configurația…');
      await this.service.validate(request);
      this.progress.set('Calculăm generațiile…');
      const result = await this.service.run(request);
      this.result.set(result);
      this.frameIndex.set(0);
      this.connected.set(true);
    } catch (error: unknown) {
      if (error instanceof HttpErrorResponse && error.status === 0) {
        this.connected.set(false);
        this.error.set('Motorul nu răspunde. Pornește backend-ul și încearcă din nou.');
      } else if (error instanceof HttpErrorResponse && typeof error.error?.error === 'string') {
        this.error.set(error.error.error);
      } else {
        this.error.set('Simularea nu a putut fi rulată. Încearcă din nou.');
      }
    } finally {
      this.running.set(false);
      this.progress.set('');
    }
  }

  goToFrame(index: number) {
    this.pause();
    this.frameIndex.set(Math.max(0, Math.min(index, this.lastFrame())));
  }

  onTimelineChange(event: Event) {
    this.goToFrame(Number((event.target as HTMLInputElement).value));
  }

  togglePlayback() {
    if (this.playing()) {
      this.pause();
      return;
    }
    if (this.lastFrame() === 0) return;
    if (this.frameIndex() === this.lastFrame()) this.frameIndex.set(0);
    this.playing.set(true);
    this.timer = setInterval(() => {
      this.frameIndex.update((index) => Math.min(index + 1, this.lastFrame()));
      if (this.frameIndex() === this.lastFrame()) this.pause();
    }, 500);
  }

  pause() {
    if (this.timer) clearInterval(this.timer);
    this.timer = undefined;
    this.playing.set(false);
  }
}
