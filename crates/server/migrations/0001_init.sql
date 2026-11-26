CREATE TABLE results (
  id               TEXT PRIMARY KEY,
  fingerprint      TEXT NOT NULL UNIQUE,
  delete_token_sha TEXT NOT NULL,
  created_at       TEXT NOT NULL,
  suite_id         TEXT NOT NULL,
  suite_version    INTEGER NOT NULL,
  client_version   TEXT NOT NULL,
  os_name          TEXT NOT NULL,
  os_version       TEXT,
  arch             TEXT NOT NULL,
  cpu_brand        TEXT NOT NULL,
  cpu_family       TEXT NOT NULL,
  cpu_vendor       TEXT,
  physical_cores   INTEGER, logical_cores INTEGER, p_cores INTEGER, e_cores INTEGER,
  mem_bytes        INTEGER NOT NULL,
  mem_gb           INTEGER NOT NULL,
  machine_vendor   TEXT, machine_family TEXT, model_id TEXT, form_factor TEXT,
  storage_kind     TEXT, filesystem TEXT,
  power_source     TEXT, low_power INTEGER, virtualized INTEGER,
  flags            TEXT NOT NULL DEFAULT '[]',
  excluded         INTEGER NOT NULL DEFAULT 0,
  hidden           INTEGER NOT NULL DEFAULT 0,
  raw_json         TEXT NOT NULL
);
CREATE TABLE scenario_results (
  result_id        TEXT NOT NULL REFERENCES results(id) ON DELETE CASCADE,
  scenario_id      TEXT NOT NULL,
  median_wall_ms   INTEGER NOT NULL,
  min_wall_ms      INTEGER NOT NULL,
  max_wall_ms      INTEGER NOT NULL,
  cv               REAL NOT NULL,
  cpu_util         REAL,
  peak_mem_delta   INTEGER,
  peak_swap        INTEGER,
  PRIMARY KEY (result_id, scenario_id)
);
CREATE INDEX idx_results_suite ON results(suite_id, suite_version, excluded, hidden);
CREATE INDEX idx_results_cpu ON results(cpu_family, mem_gb);
CREATE INDEX idx_scen ON scenario_results(scenario_id, median_wall_ms);
