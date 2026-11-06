CREATE TABLE IF NOT EXISTS simulations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    request_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS simulation_generations (
    simulation_id INTEGER NOT NULL REFERENCES simulations(id) ON DELETE CASCADE,
    generation INTEGER NOT NULL CHECK (generation >= 0),
    world_json TEXT NOT NULL,
    PRIMARY KEY (simulation_id, generation)
);

CREATE TABLE IF NOT EXISTS simulation_checkpoints (
    simulation_id INTEGER PRIMARY KEY REFERENCES simulations(id) ON DELETE CASCADE,
    state_json TEXT NOT NULL
);
