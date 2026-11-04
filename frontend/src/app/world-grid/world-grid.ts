import { Component, computed, input, output } from '@angular/core';
import { Agent, ObjectState, ObjectType, Position, World } from '../simulation.models';

const COLORS = ['#eaf0ec', '#c3e2ce', '#f4cfb1', '#d4ddf3', '#e6d7eb', '#f1dfab'];
export function typeColor(id: number) {
  return COLORS[id % COLORS.length];
}

@Component({
  selector: 'app-world-grid',
  templateUrl: './world-grid.html',
  styleUrl: './world-grid.css',
})
export class WorldGrid {
  private readonly numberFormat = new Intl.NumberFormat('ro-RO', { maximumFractionDigits: 2 });
  readonly world = input.required<World>();
  readonly types = input.required<ObjectType[]>();
  readonly selected = input<Position | null>(null);
  readonly cellSelected = output<Position>();
  readonly columns = computed(() => this.world().cells[0]?.length ?? 1);
  readonly numericProperty = computed(() => {
    for (const row of this.world().cells) {
      for (const cell of row) {
        const property = Object.keys(cell.properties)[0];
        if (property) return property;
      }
    }
    return null;
  });
  readonly numericMax = computed(() => {
    const property = this.numericProperty();
    if (!property) return 1;
    return Math.max(
      1,
      ...this.world()
        .cells.flat()
        .map((cell) => cell.properties[property] ?? 0),
    );
  });
  readonly cells = computed(() =>
    this.world().cells.flatMap((row, rowIndex) =>
      row.map((state, col) => {
        const agents = this.world().agents.filter(
          (agent) => agent.position.row === rowIndex && agent.position.col === col,
        );
        const agentType = agents[0]?.state.type_id;
        return {
          state,
          row: rowIndex,
          col,
          agents,
          agentColor: agentType === undefined ? '' : this.agentColor(agentType),
        };
      }),
    ),
  );

  name(id: number) {
    return this.types().find((type) => type.id === id)?.name ?? `tip ${id}`;
  }

  background(state: ObjectState) {
    const property = this.numericProperty();
    if (property && state.properties[property] !== undefined) {
      const strength = Math.max(0, Math.min(1, state.properties[property] / this.numericMax()));
      return `hsl(150 38% ${96 - strength * 35}%)`;
    }
    return typeColor(state.type_id);
  }

  cellText(state: ObjectState) {
    const property = this.numericProperty();
    if (property && state.properties[property] !== undefined) {
      return this.numberFormat.format(state.properties[property]);
    }
    return state.type_id === 0 ? '' : this.name(state.type_id).slice(0, 1).toUpperCase();
  }

  label(state: ObjectState, row: number, col: number, agents: Agent[]) {
    const parts = [`Rând ${row + 1}, coloană ${col + 1}: ${this.name(state.type_id)}`];
    for (const [name, value] of Object.entries(state.properties)) {
      parts.push(`${name}: ${this.numberFormat.format(value)}`);
    }
    if (agents.length === 0) {
      parts.push('Fără agenți');
    } else {
      parts.push(
        `${agents.length} agenți: ${agents.map((agent) => this.name(agent.state.type_id)).join(', ')}`,
      );
    }
    return parts.join('. ');
  }

  private agentColor(id: number) {
    return ['#31473b', '#245b8a', '#8b4518', '#594384'][id % 4];
  }
}
