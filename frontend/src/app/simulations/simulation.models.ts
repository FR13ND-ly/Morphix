export interface PropertyDefinition {
  default_value: number;
  min_value: number;
  max_value: number;
  integer_only: boolean;
}

export interface ObjectType {
  id: number;
  name: string;
  properties: Record<string, PropertyDefinition>;
}

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

export interface InitialPropertySettings {
  name: string;
  definition: PropertyDefinition;
  minimum: number | null;
  maximum: number | null;
}

export interface InitialTypeSettings {
  type_id: number;
  name: string;
  cell_weight: number | null;
  agent_count: number | null;
  agent_cell_type: number | null;
  properties: InitialPropertySettings[];
}

export type Offset = [number, number];
export type Boundary = { kind: 'wrap' } | { kind: 'fixed'; state: ObjectState };

export type Expression =
  | { kind: 'constant'; args: number }
  | { kind: 'property' | 'cell_property'; args: string }
  | { kind: 'neighbor_property'; args: { offset: Offset; property: string } }
  | { kind: 'count_neighbors' | 'count_neighbor_agents'; args: { type_id: number } }
  | { kind: 'sum_neighbors'; args: { property: string } }
  | { kind: 'sum_neighbor_agents'; args: { type_id: number; property: string } }
  | { kind: 'add' | 'subtract' | 'multiply' | 'divide' | 'power'; args: [Expression, Expression] };

export type Comparison =
  | { kind: 'equal' | 'not_equal' | 'less' | 'less_or_equal' | 'greater' | 'greater_or_equal' }
  | { kind: 'approximately_equal'; tolerance: number };

export type Condition =
  | { kind: 'always' }
  | { kind: 'type_is' | 'cell_type_is'; args: number }
  | { kind: 'neighbor_type_is'; args: { offset: Offset; type_id: number } }
  | { kind: 'compare'; args: { left: Expression; operator: Comparison; right: Expression } }
  | { kind: 'all' | 'any'; args: Condition[] }
  | { kind: 'not'; args: Condition };

export type NeighborSelection = 'first' | 'random';

export type Action =
  | { kind: 'set_property' | 'set_cell_property'; property: string; value: Expression }
  | { kind: 'set_type' | 'set_cell_type'; type_id: number }
  | { kind: 'add_neighbor_property'; offset: Offset; property: string; value: Expression }
  | { kind: 'move'; row_offset: Expression; col_offset: Expression }
  | {
      kind: 'move_to_free_neighbor';
      selection: NeighborSelection;
      cell_type?: number | null;
      offsets?: Offset[] | null;
    }
  | { kind: 'move_to_free_cell'; selection: NeighborSelection; cell_type?: number | null }
  | { kind: 'move_to_removed_neighbor' | 'remove_agent' }
  | { kind: 'remove_neighbor_agent'; type_id: number; selection: NeighborSelection }
  | {
      kind: 'spawn_agent';
      type_id: number;
      row_offset: Expression;
      col_offset: Expression;
      properties?: Record<string, Expression>;
    };

export interface Rule {
  target: 'cell' | 'agent';
  condition: Condition;
  probability: Expression | number;
  actions: Action[];
}

export interface AutomatonDefinition {
  name: string;
  object_types: ObjectType[];
  neighborhood: { offsets: Offset[] };
  boundary: Boundary;
  rules: Rule[];
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

export interface StoredSimulation {
  id: number;
  created_at: string;
  request: SimulationRequest;
  history: SimulationGeneration[];
}

export interface SimulationValidationResponse {
  valid: boolean;
}

export type SimulationSort = 'created_at' | 'name' | 'generations';
export type SortOrder = 'asc' | 'desc';

export interface SimulationListQuery {
  page: number;
  page_size: number;
  search: string;
  sort: SimulationSort;
  order: SortOrder;
}

export interface SimulationSummary {
  id: number;
  name: string;
  created_at: string;
  width: number;
  height: number;
  generations: number;
  save_history: boolean;
}

export interface SimulationListResponse {
  items: SimulationSummary[];
  total: number;
  page: number;
  page_size: number;
}
