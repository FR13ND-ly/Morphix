export interface ObjectState {
  type_id: number;
  properties: Record<string, number>;
}

export interface Position {
  row: number;
  col: number;
}

export interface Agent {
  id: number;
  position: Position;
  state: ObjectState;
}

export interface World {
  cells: ObjectState[][];
  agents: Agent[];
}

export interface ObjectType {
  id: number;
  name: string;
  properties: Record<
    string,
    {
      default_value: number;
      min_value: number;
      max_value: number;
      integer_only: boolean;
    }
  >;
}

export interface AutomatonDefinition {
  name: string;
  object_types: ObjectType[];
  neighborhood: { offsets: [number, number][] };
  boundary: { kind: 'wrap' } | { kind: 'fixed'; state: ObjectState };
  rules: unknown[];
}

export interface SimulationRequest {
  automaton: AutomatonDefinition;
  initial_world: World;
  generations: number;
  seed: number;
  save_history?: boolean;
}

export interface SimulationGeneration {
  generation: number;
  world: World;
}

export interface SimulationResponse {
  id: number;
  seed: number;
  history: SimulationGeneration[];
}
