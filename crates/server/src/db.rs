//! SQLite storage. The raw JSON is the source of truth; columns are derived on insert.

use std::str::FromStr;

use anyhow::Result;
use benchpress_core::BenchResult;
use benchpress_core::hardware::{cpu_family, cpu_vendor, memory_bucket};
use benchpress_core::result::PowerSource;
use benchpress_core::validate::Flag;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Row, SqlitePool};

/// Open (creating if needed) and migrate the database.
pub async fn open(path: &str) -> Result<SqlitePool> {
    let opts = SqliteConnectOptions::from_str(&format!("sqlite://{path}"))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(std::time::Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

/// Recompute stored flags from each result's raw JSON, so a changed rule applies to old results.
pub async fn reflag(pool: &SqlitePool, registry: &benchpress_core::Registry) -> Result<u64> {
    let rows = sqlx::query("SELECT id, flags, raw_json FROM results")
        .fetch_all(pool)
        .await?;
    let mut changed = 0;
    for row in rows {
        let Ok(r) = serde_json::from_str::<BenchResult>(row.get("raw_json")) else {
            continue;
        };
        let Ok(flags) = benchpress_core::validate::validate_result(&r, registry) else {
            continue;
        };
        let names: Vec<&str> = flags.iter().map(|f| f.as_str()).collect();
        let names = serde_json::to_string(&names)?;
        if names != row.get::<&str, _>("flags") {
            sqlx::query("UPDATE results SET flags = ?, excluded = ? WHERE id = ?")
                .bind(&names)
                .bind(i64::from(!flags.is_empty()))
                .bind(row.get::<&str, _>("id"))
                .execute(pool)
                .await?;
            changed += 1;
        }
    }
    Ok(changed)
}

/// Outcome of an insert.
pub enum Inserted {
    /// Stored.
    Ok,
    /// Same fingerprint already stored.
    Duplicate,
    /// The generated public id already exists; retry with a new one.
    IdTaken,
}

/// Store a validated result.
pub async fn insert(
    pool: &SqlitePool,
    id: &str,
    fingerprint: &str,
    delete_token_sha: &str,
    created_at: &str,
    r: &BenchResult,
    flags: &[Flag],
) -> Result<Inserted> {
    let hw = &r.hardware;
    let env = &r.environment;
    let flag_names: Vec<&str> = flags.iter().map(|f| f.as_str()).collect();
    let mut tx = pool.begin().await?;
    let res = sqlx::query(
        "INSERT INTO results (id, fingerprint, delete_token_sha, created_at, suite_id, suite_version, client_version,
           os_name, os_version, arch, cpu_brand, cpu_family, cpu_vendor, physical_cores, logical_cores, p_cores, e_cores,
           mem_bytes, mem_gb, machine_vendor, machine_family, model_id, form_factor, storage_kind, filesystem,
           power_source, low_power, virtualized, flags, excluded, hidden, raw_json)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,0,?)",
    )
    .bind(id)
    .bind(fingerprint)
    .bind(delete_token_sha)
    .bind(created_at)
    .bind(&r.suite.id)
    .bind(i64::from(r.suite.version))
    .bind(&r.client.version)
    .bind(&env.os.name)
    .bind(&env.os.version)
    .bind(&hw.cpu.arch)
    .bind(&hw.cpu.brand)
    .bind(cpu_family(&hw.cpu.brand))
    .bind(hw.cpu.vendor.clone().or_else(|| cpu_vendor(&hw.cpu.brand).map(str::to_owned)))
    .bind(hw.cpu.physical_cores.map(i64::from))
    .bind(i64::from(hw.cpu.logical_cores))
    .bind(hw.cpu.performance_cores.map(i64::from))
    .bind(hw.cpu.efficiency_cores.map(i64::from))
    .bind(hw.memory.total_bytes as i64)
    .bind(i64::from(memory_bucket(hw.memory.total_bytes)))
    .bind(&hw.machine.vendor)
    .bind(&hw.machine.family)
    .bind(&hw.machine.model_id)
    .bind(&hw.machine.form_factor)
    .bind(&hw.storage.kind)
    .bind(&hw.storage.filesystem)
    .bind(env.power.source.map(|s| match s {
        PowerSource::Ac => "ac",
        PowerSource::Battery => "battery",
    }))
    .bind(env.power.low_power_mode.map(i64::from))
    .bind(i64::from(env.virtualization.is_some()))
    .bind(serde_json::to_string(&flag_names)?)
    .bind(i64::from(!flags.is_empty()))
    .bind(serde_json::to_string(r)?)
    .execute(&mut *tx)
    .await;
    match res {
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Ok(if e.message().contains("results.id") {
                Inserted::IdTaken
            } else {
                Inserted::Duplicate
            });
        }
        Err(e) => return Err(e.into()),
        Ok(_) => {}
    }
    for s in &r.scenarios {
        sqlx::query(
            "INSERT INTO scenario_results (result_id, scenario_id, median_wall_ms, min_wall_ms, max_wall_ms, cv, cpu_util, peak_mem_delta, peak_swap)
             VALUES (?,?,?,?,?,?,?,?,?)",
        )
        .bind(id)
        .bind(&s.id)
        .bind(s.summary.median_wall_ms as i64)
        .bind(s.summary.min_wall_ms as i64)
        .bind(s.summary.max_wall_ms as i64)
        .bind(s.summary.cv)
        .bind(s.cpu_util())
        .bind(s.peak_mem_delta() as i64)
        .bind(s.peak_swap_delta() as i64)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(Inserted::Ok)
}

/// A stored result as served publicly.
pub struct Stored {
    /// Public id.
    pub id: String,
    /// RFC 3339 insert time.
    pub created_at: String,
    /// Validation flags.
    pub flags: Vec<Flag>,
    /// Raw document.
    pub result: BenchResult,
}

/// Fetch a visible result.
pub async fn get(pool: &SqlitePool, id: &str) -> Result<Option<Stored>> {
    let row = sqlx::query(
        "SELECT id, created_at, flags, raw_json FROM results WHERE id = ? AND hidden = 0",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else { return Ok(None) };
    Ok(Some(Stored {
        id: row.get("id"),
        created_at: row.get("created_at"),
        flags: serde_json::from_str(row.get::<&str, _>("flags")).unwrap_or_default(),
        result: serde_json::from_str(row.get::<&str, _>("raw_json"))?,
    }))
}

/// Delete token hash of a visible result.
pub async fn delete_token_sha(pool: &SqlitePool, id: &str) -> Result<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT delete_token_sha FROM results WHERE id = ? AND hidden = 0")
            .bind(id)
            .fetch_optional(pool)
            .await?,
    )
}

/// Delete a result and its scenario rows.
pub async fn delete(pool: &SqlitePool, id: &str) -> Result<bool> {
    let r = sqlx::query("DELETE FROM results WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(r.rows_affected() > 0)
}

/// Summary row used by listings and aggregates.
#[derive(Debug, Clone, Default)]
pub struct ResultRow {
    pub id: String,
    pub created_at: String,
    pub suite_id: String,
    pub suite_version: u32,
    pub os_name: String,
    pub arch: String,
    pub cpu_family: String,
    pub cpu_vendor: Option<String>,
    pub logical_cores: Option<u32>,
    pub p_cores: Option<u32>,
    pub e_cores: Option<u32>,
    pub mem_gb: u32,
    pub machine_vendor: Option<String>,
    pub machine_family: Option<String>,
    pub form_factor: Option<String>,
    pub excluded: bool,
    pub outlier: bool,
    pub seed: bool,
    pub scenarios: Vec<ScenarioRow>,
}

/// Per-scenario columns.
#[derive(Debug, Clone, Default)]
pub struct ScenarioRow {
    pub scenario_id: String,
    pub median_wall_ms: u64,
    pub min_wall_ms: u64,
    pub max_wall_ms: u64,
    pub cv: f64,
    pub cpu_util: Option<f64>,
    pub peak_mem_delta: Option<u64>,
    pub peak_swap: Option<u64>,
}

impl ResultRow {
    /// Scenario columns by id.
    pub fn scenario(&self, id: &str) -> Option<&ScenarioRow> {
        self.scenarios.iter().find(|s| s.scenario_id == id)
    }
}

/// Every visible result with its scenarios, newest first.
pub async fn all_rows(pool: &SqlitePool) -> Result<Vec<ResultRow>> {
    let rows = sqlx::query(
        "SELECT id, created_at, suite_id, suite_version, os_name, arch, cpu_family, cpu_vendor, logical_cores, p_cores, e_cores,
                mem_gb, machine_vendor, machine_family, form_factor, excluded, client_version
         FROM results WHERE hidden = 0 ORDER BY created_at DESC, id",
    )
    .fetch_all(pool)
    .await?;
    let scen = sqlx::query(
        "SELECT s.result_id, s.scenario_id, s.median_wall_ms, s.min_wall_ms, s.max_wall_ms, s.cv, s.cpu_util, s.peak_mem_delta, s.peak_swap
         FROM scenario_results s JOIN results r ON r.id = s.result_id WHERE r.hidden = 0",
    )
    .fetch_all(pool)
    .await?;
    let mut by_id: std::collections::HashMap<String, Vec<ScenarioRow>> =
        std::collections::HashMap::new();
    for s in scen {
        by_id
            .entry(s.get("result_id"))
            .or_default()
            .push(ScenarioRow {
                scenario_id: s.get("scenario_id"),
                median_wall_ms: s.get::<i64, _>("median_wall_ms") as u64,
                min_wall_ms: s.get::<i64, _>("min_wall_ms") as u64,
                max_wall_ms: s.get::<i64, _>("max_wall_ms") as u64,
                cv: s.get("cv"),
                cpu_util: s.get("cpu_util"),
                peak_mem_delta: s.get::<Option<i64>, _>("peak_mem_delta").map(|v| v as u64),
                peak_swap: s.get::<Option<i64>, _>("peak_swap").map(|v| v as u64),
            });
    }
    let opt_u32 =
        |r: &sqlx::sqlite::SqliteRow, c: &str| r.get::<Option<i64>, _>(c).map(|v| v as u32);
    Ok(rows
        .into_iter()
        .map(|r| {
            let id: String = r.get("id");
            ResultRow {
                scenarios: by_id.remove(&id).unwrap_or_default(),
                id,
                created_at: r.get("created_at"),
                suite_id: r.get("suite_id"),
                suite_version: r.get::<i64, _>("suite_version") as u32,
                os_name: r.get("os_name"),
                arch: r.get("arch"),
                cpu_family: r.get("cpu_family"),
                cpu_vendor: r.get("cpu_vendor"),
                logical_cores: opt_u32(&r, "logical_cores"),
                p_cores: opt_u32(&r, "p_cores"),
                e_cores: opt_u32(&r, "e_cores"),
                mem_gb: r.get::<i64, _>("mem_gb") as u32,
                machine_vendor: r.get("machine_vendor"),
                machine_family: r.get("machine_family"),
                form_factor: r.get("form_factor"),
                excluded: r.get::<i64, _>("excluded") != 0,
                outlier: false,
                seed: r.get::<String, _>("client_version").ends_with("-seed"),
            }
        })
        .collect())
}
