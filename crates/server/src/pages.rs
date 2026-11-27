//! Server-rendered HTML pages.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use benchpress_core::format::duration_ms;
use benchpress_core::hardware::{cpu_family, memory_bucket};
use benchpress_core::suite::scenario_label;
use benchpress_core::{RegisteredSuite, Tier};
use minijinja::Value;
use serde_json::{Map, json};
use sha2::{Digest, Sha256};

use crate::charts::{self, Bar, Mirrored, Panel, Point};
use crate::stats::{ConfigKey, ConfigStats, Filter, Snapshot};
use crate::{AppState, assets, db, og};

type Q = Query<HashMap<String, String>>;

const HEADLINE: &str = "clean-release";
const PAGE_SIZE: usize = 25;

/// Page routes.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(home))
        .route("/machines", get(machines))
        .route("/r/{id}", get(result))
        .route("/r/{id}/og.png", get(og_image))
        .route("/compare", get(compare))
        .route("/insights/memory", get(insights_memory))
        .route("/insights/cores", get(insights_cores))
        .route("/suites", get(suites))
        .route("/about", get(about))
        .route("/start", get(start))
        .route("/theme", get(theme))
}

struct Page<'a> {
    state: &'a AppState,
    jar: &'a CookieJar,
    headers: &'a HeaderMap,
}

impl Page<'_> {
    fn base(
        &self,
        title: &str,
        description: &str,
        path: &str,
        nav: &str,
    ) -> Map<String, serde_json::Value> {
        let theme = self
            .jar
            .get("theme")
            .map(|c| c.value().to_owned())
            .filter(|t| t == "light" || t == "dark");
        let snap = self.state.snap();
        let mut m = Map::new();
        m.insert("title".into(), title.into());
        m.insert("description".into(), description.into());
        m.insert("path".into(), path.into());
        m.insert("nav".into(), nav.into());
        m.insert("theme".into(), theme.into());
        m.insert("seed".into(), snap.rows.iter().any(|r| r.seed).into());
        m.insert("css".into(), assets::url("app.css").into());
        m.insert(
            "font".into(),
            assets::url("fonts/archivo-latin-variable.woff2").into(),
        );
        m
    }

    fn render(&self, template: &str, ctx: &Map<String, serde_json::Value>) -> Response {
        self.render_status(StatusCode::OK, template, ctx)
    }

    fn render_status(
        &self,
        status: StatusCode,
        template: &str,
        ctx: &Map<String, serde_json::Value>,
    ) -> Response {
        match self
            .state
            .views
            .render(template, Value::from(minijinja::value::Serde(&ctx)))
        {
            Ok(html) => html_response(status, html, self.headers),
            Err(e) => {
                tracing::error!(error = %e, template, "render failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "The page could not be rendered.",
                )
                    .into_response()
            }
        }
    }
}

fn html_response(status: StatusCode, html: String, headers: &HeaderMap) -> Response {
    let etag = format!(
        "W/\"{}\"",
        hex::encode(&Sha256::digest(html.as_bytes())[..8])
    );
    if status == StatusCode::OK
        && headers
            .get(header::IF_NONE_MATCH)
            .and_then(|v| v.to_str().ok())
            == Some(etag.as_str())
    {
        return (StatusCode::NOT_MODIFIED, [(header::ETAG, etag)]).into_response();
    }
    let mut res = (status, html).into_response();
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=60"),
    );
    h.insert(header::VARY, HeaderValue::from_static("Cookie"));
    if let Ok(v) = HeaderValue::from_str(&etag) {
        h.insert(header::ETAG, v);
    }
    res
}

fn tier_json(t: Tier) -> serde_json::Value {
    json!({ "id": t.as_str(), "label": t.label() })
}

fn suite_json(s: &RegisteredSuite, n: usize) -> serde_json::Value {
    let m = &s.manifest;
    json!({
        "id": m.id, "version": m.version, "key": m.key(), "tier": tier_json(m.tier), "display_name": m.display_name,
        "description": m.description, "project": m.project(), "repo": m.source.repo, "commit": m.source.commit,
        "commit_short": &m.source.commit[..7], "archive_url": m.source.archive_url(), "archive_sha256": m.source.archive_sha256,
        "toolchain": m.toolchain.channel, "hash": s.hash, "runs": m.runs.count, "disk_gb": m.requirements.disk_gb,
        "minutes": format!("{} to {} minutes", m.requirements.estimated_minutes.min, m.requirements.estimated_minutes.max),
        "build_args": m.build.cargo_args().join(" "),
        "scenarios": m.scenarios.iter().map(|sc| json!({
            "id": sc.id, "label": sc.label(), "kind": match sc.kind { benchpress_core::suite::ScenarioKind::Clean => "clean", benchpress_core::suite::ScenarioKind::Incremental => "incremental" },
            "args": sc.args.join(" "), "touch": sc.touch,
        })).collect::<Vec<_>>(),
        "n": n,
    })
}

/// Resolve `suite` query values like `rust-standard` or `rust-standard@1`; default is the standard tier.
fn pick_suite<'a>(state: &'a AppState, q: Option<&String>) -> &'a RegisteredSuite {
    let reg = &state.registry;
    q.and_then(|s| match s.split_once('@') {
        Some((id, v)) => v.parse().ok().and_then(|v| reg.get(id, v)),
        None => reg.latest(s),
    })
    .or_else(|| reg.by_tier(Tier::Standard))
    .or_else(|| reg.latest_all().into_iter().next())
    .expect("at least one builtin suite")
}

fn config_href(suite: &RegisteredSuite, scenario: &str, k: &ConfigKey) -> String {
    format!(
        "/machines?suite={}&scenario={}&cpu={}&ram={}&os={}",
        enc(&suite.manifest.key()),
        enc(scenario),
        enc(&k.cpu_family),
        k.mem_gb,
        enc(&k.os_name)
    )
}

fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'@') {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

fn config_json(
    c: &ConfigStats,
    scale: u64,
    suite: &RegisteredSuite,
    scenario: &str,
) -> serde_json::Value {
    json!({
        "cpu": c.key.cpu_family, "mem_gb": c.key.mem_gb, "os": c.key.os_name, "n": c.n,
        "median": duration_ms(c.median), "p25": duration_ms(c.p25), "p75": duration_ms(c.p75),
        "range_bar": charts::range_bar(c.p25, c.median, c.p75, scale), "token": c.key.token(),
        "families": c.families.join(", "), "href": config_href(suite, scenario, &c.key),
        "single": if c.n == 1 { c.result_ids.first().cloned() } else { None },
    })
}

fn latest_json(snap: &Snapshot, state: &AppState, limit: usize) -> Vec<serde_json::Value> {
    snap.rows
        .iter()
        .take(limit)
        .map(|r| {
            let suite = state.registry.get(&r.suite_id, r.suite_version);
            let s = r.scenario(HEADLINE).or(r.scenarios.first());
            json!({
                "id": r.id,
                "machine": ([r.machine_family.clone(), Some(r.cpu_family.clone()), Some(format!("{} GB", r.mem_gb))].into_iter().flatten().collect::<Vec<_>>().join(" · ")),
                "os": r.os_name, "tier": suite.map(|s| tier_json(s.manifest.tier)),
                "time": s.map(|s| duration_ms(s.median_wall_ms)), "scenario": s.map(|s| scenario_label(&s.scenario_id)),
                "date": r.created_at.get(..10), "excluded": r.excluded,
            })
        })
        .collect()
}

async fn home(State(state): State<Arc<AppState>>, jar: CookieJar, headers: HeaderMap) -> Response {
    let p = Page {
        state: &state,
        jar: &jar,
        headers: &headers,
    };
    let snap = state.snap();
    let suite = pick_suite(&state, None);
    let m = &suite.manifest;
    let configs = snap.configs(&m.id, m.version, HEADLINE, &Filter::default());
    let bars: Vec<Bar> = configs
        .iter()
        .take(10)
        .map(|c| Bar {
            label: c.key.cpu_family.clone(),
            sublabel: format!("{} GB · {}", c.key.mem_gb, c.key.os_name),
            value_ms: c.median,
            href: Some(c.result_ids.first().filter(|_| c.n == 1).map_or_else(
                || config_href(suite, HEADLINE, &c.key),
                |id| format!("/r/{id}"),
            )),
            note: if c.n == 1 {
                "1 result".into()
            } else {
                format!("{} results", c.n)
            },
        })
        .collect();
    let chart = (!bars.is_empty()).then(|| {
        charts::ranked_bars(
            &format!("Fastest machines building {}", m.project()),
            "Median clean release build time per configuration",
            &bars,
        )
    });
    let tiers: Vec<_> = state
        .registry
        .latest_all()
        .into_iter()
        .map(|s| {
            suite_json(
                s,
                snap.rows
                    .iter()
                    .filter(|r| {
                        r.suite_id == s.manifest.id && r.suite_version == s.manifest.version
                    })
                    .count(),
            )
        })
        .collect();
    let mut ctx = p.base(
        "Benchpress",
        "Know what your machine can lift. Compile-time benchmarks from real machines.",
        "/",
        "home",
    );
    ctx.insert("chart".into(), chart.into());
    ctx.insert(
        "suite".into(),
        suite_json(suite, configs.iter().map(|c| c.n).sum()),
    );
    ctx.insert("tiers".into(), tiers.into());
    ctx.insert("latest".into(), latest_json(&snap, &state, 8).into());
    ctx.insert("total".into(), snap.rows.len().into());
    p.render("home.html", &ctx)
}

#[derive(Clone, Copy, PartialEq)]
enum Sort {
    Median,
    N,
    Cpu,
    Ram,
}

async fn machines(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    headers: HeaderMap,
    Query(q): Q,
) -> Response {
    let p = Page {
        state: &state,
        jar: &jar,
        headers: &headers,
    };
    let snap = state.snap();
    let suite = pick_suite(&state, q.get("suite"));
    let m = &suite.manifest;
    let scenario = q
        .get("scenario")
        .filter(|s| m.scenario(s).is_some())
        .cloned()
        .unwrap_or_else(|| HEADLINE.into());
    let nonempty = |k: &str| q.get(k).filter(|v| !v.is_empty()).cloned();
    let filter = Filter {
        os: nonempty("os"),
        vendor: nonempty("vendor"),
        mem_gb: nonempty("ram").and_then(|v| v.parse().ok()),
        form_factor: nonempty("form"),
        include_flagged: q.get("flagged").is_some_and(|v| v == "1" || v == "on"),
    };
    let cpu = nonempty("cpu");
    let mut configs: Vec<ConfigStats> = snap
        .configs(&m.id, m.version, &scenario, &filter)
        .into_iter()
        .filter(|c| cpu.as_ref().is_none_or(|f| &c.key.cpu_family == f))
        .collect();
    let sort = match q.get("sort").map(String::as_str) {
        Some("n") => Sort::N,
        Some("cpu") => Sort::Cpu,
        Some("ram") => Sort::Ram,
        _ => Sort::Median,
    };
    let desc = q.get("dir").is_some_and(|d| d == "desc");
    configs.sort_by(|a, b| {
        let o = match sort {
            Sort::Median => a.median.cmp(&b.median),
            Sort::N => a.n.cmp(&b.n),
            Sort::Cpu => a.key.cpu_family.cmp(&b.key.cpu_family),
            Sort::Ram => a.key.mem_gb.cmp(&b.key.mem_gb),
        }
        .then_with(|| a.median.cmp(&b.median));
        if desc { o.reverse() } else { o }
    });
    let scale = configs.iter().map(|c| c.p75).max().unwrap_or(1);
    let pages = configs.len().div_ceil(PAGE_SIZE).max(1);
    let page = q
        .get("page")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1)
        .clamp(1, pages);
    let rows: Vec<_> = configs
        .iter()
        .skip((page - 1) * PAGE_SIZE)
        .take(PAGE_SIZE)
        .map(|c| config_json(c, scale, suite, &scenario))
        .collect();

    let suite_rows: Vec<_> = snap
        .rows
        .iter()
        .filter(|r| r.suite_id == m.id && r.suite_version == m.version)
        .collect();
    let distinct = |f: &dyn Fn(&&db::ResultRow) -> Option<String>| -> Vec<String> {
        suite_rows
            .iter()
            .filter_map(f)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    };
    let mut rams: Vec<u32> = suite_rows
        .iter()
        .map(|r| r.mem_gb)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    rams.sort_unstable();
    let mut qs: BTreeMap<String, String> = q
        .iter()
        .filter(|(k, v)| !v.is_empty() && *k != "page" && *k != "sort" && *k != "dir")
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    qs.insert("suite".into(), m.key());
    qs.insert("scenario".into(), scenario.clone());
    let base_qs: String = qs
        .iter()
        .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
        .collect::<Vec<_>>()
        .join("&");
    let sort_link = |key: &str, default_desc: bool| {
        let is_current = match key {
            "median" => sort == Sort::Median,
            "n" => sort == Sort::N,
            "cpu" => sort == Sort::Cpu,
            _ => sort == Sort::Ram,
        };
        let dir = if is_current {
            if desc { "asc" } else { "desc" }
        } else if default_desc {
            "desc"
        } else {
            "asc"
        };
        json!({
            "href": format!("/machines?{base_qs}&sort={key}&dir={dir}"),
            "aria": if is_current { if desc { "descending" } else { "ascending" } } else { "none" },
        })
    };
    let mut ctx = p.base(
        "Machines",
        &format!("Compare {} build times by CPU, RAM and OS.", m.project()),
        "/machines",
        "machines",
    );
    ctx.insert("suite".into(), suite_json(suite, suite_rows.len()));
    ctx.insert("suites".into(), state.registry.all().map(|s| json!({"key": s.manifest.key(), "label": format!("{} · {}@{}", s.manifest.display_name, s.manifest.project(), s.manifest.version)})).collect::<Vec<_>>().into());
    ctx.insert("scenario".into(), scenario.clone().into());
    ctx.insert("scenario_label".into(), scenario_label(&scenario).into());
    ctx.insert(
        "scenarios".into(),
        m.scenarios
            .iter()
            .map(|s| json!({"id": s.id, "label": s.label()}))
            .collect::<Vec<_>>()
            .into(),
    );
    ctx.insert("oses".into(), distinct(&|r| Some(r.os_name.clone())).into());
    ctx.insert("vendors".into(), distinct(&|r| r.cpu_vendor.clone()).into());
    ctx.insert("forms".into(), distinct(&|r| r.form_factor.clone()).into());
    ctx.insert("rams".into(), rams.into());
    ctx.insert("q".into(), json!({
        "os": filter.os, "vendor": filter.vendor, "ram": filter.mem_gb, "form": filter.form_factor,
        "flagged": filter.include_flagged, "cpu": cpu,
    }));
    ctx.insert("rows".into(), rows.into());
    ctx.insert("count".into(), configs.len().into());
    ctx.insert(
        "results".into(),
        configs.iter().map(|c| c.n).sum::<usize>().into(),
    );
    ctx.insert("sort".into(), json!({
        "median": sort_link("median", false), "n": sort_link("n", true), "cpu": sort_link("cpu", false), "ram": sort_link("ram", false),
    }));
    let page_link = |n: usize| {
        format!(
            "/machines?{base_qs}&sort={}&dir={}&page={n}",
            q.get("sort").map_or("median", String::as_str),
            if desc { "desc" } else { "asc" }
        )
    };
    ctx.insert("page".into(), json!({
        "current": page, "total": pages,
        "prev": (page > 1).then(|| page_link(page - 1)), "next": (page < pages).then(|| page_link(page + 1)),
    }));
    p.render("machines.html", &ctx)
}

fn os_label(r: &benchpress_core::BenchResult) -> String {
    let os = &r.environment.os;
    match &os.version {
        Some(v) if os.name == "Linux" => v.clone(),
        Some(v) => format!("{} {v}", os.name),
        None => os.name.clone(),
    }
}

fn power_label(r: &benchpress_core::BenchResult) -> String {
    use benchpress_core::result::PowerSource;
    let p = &r.environment.power;
    let mut s = match p.source {
        Some(PowerSource::Ac) => "AC power".to_owned(),
        Some(PowerSource::Battery) => "Battery".to_owned(),
        None => "Unknown".to_owned(),
    };
    if p.low_power_mode == Some(true) {
        s.push_str(", low power mode");
    }
    if let Some(profile) = &p.profile {
        let _ = write!(s, ", {profile} profile");
    }
    s
}

async fn result(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let p = Page {
        state: &state,
        jar: &jar,
        headers: &headers,
    };
    let stored = match db::get(&state.pool, &id).await {
        Ok(Some(s)) => s,
        Ok(None) => return not_found_page(&p),
        Err(e) => {
            tracing::error!(error = %e, "loading result");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "The result could not be loaded.",
            )
                .into_response();
        }
    };
    let r = &stored.result;
    let Some(suite) = state.registry.get(&r.suite.id, r.suite.version) else {
        return not_found_page(&p);
    };
    let m = &suite.manifest;
    let snap = state.snap();
    let mut flags = stored.flags.clone();
    if snap.row(&stored.id).is_some_and(|row| row.outlier) {
        flags.push(benchpress_core::validate::Flag::Outlier);
    }
    let headline = r
        .scenario(HEADLINE)
        .or(r.scenarios.first())
        .map(|s| s.id.clone())
        .unwrap_or_default();
    let this = r
        .scenario(&headline)
        .map_or(0, |s| s.summary.median_wall_ms);
    let others: Vec<u64> = snap
        .rows_for(&m.id, m.version, &headline, &Filter::default())
        .filter(|row| row.id != stored.id)
        .filter_map(|row| row.scenario(&headline).map(|s| s.median_wall_ms))
        .collect();
    let slower = others.iter().filter(|v| **v > this).count();
    let comparison = if others.is_empty() {
        "The first result for this suite. Others will appear here as they come in.".to_owned()
    } else {
        format!(
            "Faster than {} of {} other results for {} on {}.",
            benchpress_core::format::percent(slower as f64 / others.len() as f64),
            others.len(),
            m.project(),
            scenario_label(&headline).to_lowercase()
        )
    };
    let strip = charts::dot_strip(
        &format!("{} times for {}", scenario_label(&headline), m.project()),
        &others,
        Some(this),
    );
    let cpu = &r.hardware.cpu;
    let mem = &r.hardware.memory;
    let machine = [
        r.hardware.machine.family.clone(),
        Some(cpu_family(&cpu.brand)),
        Some(format!("{} GB", memory_bucket(mem.total_bytes))),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    let scen: Vec<_> = r
        .scenarios
        .iter()
        .map(|s| {
            json!({
                "id": s.id, "label": scenario_label(&s.id), "median": duration_ms(s.summary.median_wall_ms),
                "min": duration_ms(s.summary.min_wall_ms), "max": duration_ms(s.summary.max_wall_ms),
                "cv": format!("{:.1}%", s.summary.cv * 100.0), "cpu": s.cpu_util().map(benchpress_core::format::percent),
                "mem": benchpress_core::format::bytes(s.peak_mem_delta()),
                "swap": (s.peak_swap_delta() > 0).then(|| benchpress_core::format::bytes(s.peak_swap_delta())),
                "runs": s.runs.iter().map(|run| duration_ms(run.wall_ms)).collect::<Vec<_>>().join(", "),
            })
        })
        .collect();
    let cores = match (cpu.performance_cores, cpu.efficiency_cores) {
        (Some(pc), Some(ec)) => format!(
            "{} logical · {pc} performance, {ec} efficiency",
            cpu.logical_cores
        ),
        _ => match cpu.physical_cores {
            Some(ph) if ph != cpu.logical_cores => {
                format!("{ph} cores, {} threads", cpu.logical_cores)
            }
            _ => format!("{} cores", cpu.logical_cores),
        },
    };
    let opt = |v: &Option<String>| v.clone().unwrap_or_else(|| "Unknown".into());
    let env = &r.environment;
    let details = vec![
        ("CPU", cpu.brand.clone()),
        ("Cores", cores),
        (
            "Memory",
            format!(
                "{} GB{}",
                memory_bucket(mem.total_bytes),
                if mem.unified { " unified" } else { "" }
            ),
        ),
        ("Machine", {
            let mut parts: Vec<String> = [
                r.hardware.machine.vendor.clone(),
                r.hardware.machine.family.clone(),
                r.hardware.machine.model_id.clone(),
            ]
            .into_iter()
            .flatten()
            .collect();
            parts.dedup();
            parts.join(" · ")
        }),
        (
            "Storage",
            [
                r.hardware.storage.kind.clone(),
                r.hardware.storage.model.clone(),
                r.hardware.storage.filesystem.clone(),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · "),
        ),
        ("OS", os_label(r)),
        ("Kernel", opt(&env.os.kernel)),
        ("Power", power_label(r)),
        (
            "Thermal",
            env.thermal_at_start
                .map_or("Not reported".into(), |t| format!("{t:?}").to_lowercase()),
        ),
        (
            "Virtualization",
            env.virtualization
                .clone()
                .unwrap_or_else(|| "None detected".into()),
        ),
        ("Toolchain", format!("Rust {}", m.toolchain.channel)),
        (
            "Runs",
            format!(
                "{} per scenario, {}s cooldown",
                r.settings.runs, r.settings.cooldown_secs
            ),
        ),
        (
            "Shell overrides ignored",
            if env.overrides_cleared.is_empty() {
                "None".into()
            } else {
                env.overrides_cleared.join(", ")
            },
        ),
        (
            "Client",
            format!("benchpress {} · {}", r.client.version, r.client.target),
        ),
        ("Started", r.started_at.clone()),
    ];
    let all = Filter::default();
    let nearby: Vec<_> = {
        let mut v: Vec<_> = snap
            .rows_for(&m.id, m.version, &headline, &all)
            .filter(|row| row.id != stored.id)
            .filter_map(|row| row.scenario(&headline).map(|s| (row, s.median_wall_ms)))
            .collect();
        v.sort_by_key(|(_, t)| t.abs_diff(this));
        v.into_iter()
            .take(4)
            .map(|(row, t)| json!({"id": row.id, "label": format!("{} · {} GB · {}", row.cpu_family, row.mem_gb, row.os_name), "time": duration_ms(t)}))
            .collect()
    };
    let title = format!(
        "{} · {} in {}",
        machine,
        scenario_label(&headline).to_lowercase(),
        duration_ms(this)
    );
    let mut ctx = p.base(
        &title,
        &format!(
            "{} built {} ({}) in {}. {}",
            machine,
            m.project(),
            scenario_label(&headline).to_lowercase(),
            duration_ms(this),
            comparison
        ),
        &format!("/r/{}", stored.id),
        "",
    );
    ctx.insert(
        "og_image".into(),
        format!("{}/r/{}/og.png", state.config.public_url, stored.id).into(),
    );
    ctx.insert("id".into(), stored.id.clone().into());
    ctx.insert("time".into(), duration_ms(this).into());
    ctx.insert("headline".into(), scenario_label(&headline).into());
    ctx.insert("machine".into(), machine.into());
    ctx.insert("os".into(), os_label(r).into());
    ctx.insert("suite".into(), suite_json(suite, others.len() + 1));
    ctx.insert("strip".into(), strip.into());
    ctx.insert("comparison".into(), comparison.into());
    ctx.insert("scenarios".into(), scen.into());
    ctx.insert(
        "hints".into(),
        benchpress_core::hints::hints(r, &flags).into(),
    );
    ctx.insert(
        "flags".into(),
        flags
            .iter()
            .map(|f| json!({"id": f.as_str(), "text": f.explain()}))
            .collect::<Vec<_>>()
            .into(),
    );
    ctx.insert(
        "details".into(),
        details
            .into_iter()
            .map(|(k, v)| json!({"k": k, "v": v}))
            .collect::<Vec<_>>()
            .into(),
    );
    ctx.insert("nearby".into(), nearby.into());
    ctx.insert(
        "date".into(),
        stored.created_at.get(..10).unwrap_or_default().into(),
    );
    p.render("result.html", &ctx)
}

async fn og_image(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    let Ok(Some(stored)) = db::get(&state.pool, &id).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if let Some(png) = state.og_cache.get(&id) {
        return png_response(png);
    }
    let snap = state.snap();
    let st = state.clone();
    let rendered =
        tokio::task::spawn_blocking(move || og::render(&st.registry, &snap, &stored)).await;
    match rendered {
        Ok(Ok(png)) => {
            let png = state.og_cache.put(&id, png);
            if matches!(db::get(&state.pool, &id).await, Ok(Some(_))) {
                png_response(png)
            } else {
                state.og_cache.forget(&id);
                StatusCode::NOT_FOUND.into_response()
            }
        }
        Ok(Err(e)) => {
            tracing::error!(error = %e, "og render failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "og task failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn png_response(png: axum::body::Bytes) -> Response {
    (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        png,
    )
        .into_response()
}

struct Side {
    name: String,
    href: String,
    suite: (String, u32),
    medians: HashMap<String, u64>,
}

async fn resolve_side(
    state: &AppState,
    snap: &Snapshot,
    token: &str,
    suite: &RegisteredSuite,
) -> Option<Side> {
    let token = token.trim().trim_end_matches('/');
    let token = token.rsplit("/r/").next().unwrap_or(token);
    if let Some(key) = ConfigKey::parse(token) {
        let m = &suite.manifest;
        let mut medians = HashMap::new();
        for sc in &m.scenarios {
            if let Some(c) = snap
                .configs(&m.id, m.version, &sc.id, &Filter::default())
                .into_iter()
                .find(|c| c.key == key)
            {
                medians.insert(sc.id.clone(), c.median);
            }
        }
        if medians.is_empty() {
            return None;
        }
        return Some(Side {
            name: key.label(),
            href: config_href(suite, HEADLINE, &key),
            suite: (m.id.clone(), m.version),
            medians,
        });
    }
    let stored = db::get(&state.pool, token).await.ok()??;
    let r = &stored.result;
    let name = [
        r.hardware.machine.family.clone(),
        Some(cpu_family(&r.hardware.cpu.brand)),
        Some(format!(
            "{} GB",
            memory_bucket(r.hardware.memory.total_bytes)
        )),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    Some(Side {
        name,
        href: format!("/r/{}", stored.id),
        suite: (r.suite.id.clone(), r.suite.version),
        medians: r
            .scenarios
            .iter()
            .map(|s| (s.id.clone(), s.summary.median_wall_ms))
            .collect(),
    })
}

/// "B builds 34% faster on clean release (12.3s less)."
fn phrase(a: &str, b: &str, scenario: &str, ta: u64, tb: u64) -> String {
    if ta == tb {
        return format!("Both build {} in the same time.", scenario.to_lowercase());
    }
    let (fast, slow, tf, ts) = if tb < ta {
        (b, a, tb, ta)
    } else {
        (a, b, ta, tb)
    };
    let pct = (ts - tf) as f64 / ts as f64 * 100.0;
    let _ = slow;
    format!(
        "{fast} builds {pct:.0}% faster on {} ({} less).",
        scenario.to_lowercase(),
        duration_ms(ts - tf)
    )
}

async fn compare(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    headers: HeaderMap,
    Query(q): Q,
) -> Response {
    let p = Page {
        state: &state,
        jar: &jar,
        headers: &headers,
    };
    let snap = state.snap();
    let suite = pick_suite(&state, q.get("suite"));
    let mut ctx = p.base(
        "Compare",
        "Compare two Benchpress results or machine configurations side by side.",
        "/compare",
        "compare",
    );
    ctx.insert(
        "a_input".into(),
        q.get("a").cloned().unwrap_or_default().into(),
    );
    ctx.insert(
        "b_input".into(),
        q.get("b").cloned().unwrap_or_default().into(),
    );
    let (Some(a), Some(b)) = (
        q.get("a").filter(|v| !v.is_empty()),
        q.get("b").filter(|v| !v.is_empty()),
    ) else {
        return p.render("compare.html", &ctx);
    };
    let (sa, sb) = (
        resolve_side(&state, &snap, a, suite).await,
        resolve_side(&state, &snap, b, suite).await,
    );
    let (Some(sa), Some(sb)) = (sa, sb) else {
        ctx.insert("error".into(), "One of those results or configurations was not found. Paste a result link like /r/AbC123xyz9, or pick a configuration from the machines page.".into());
        return p.render_status(StatusCode::NOT_FOUND, "compare.html", &ctx);
    };
    if sa.suite != sb.suite {
        ctx.insert("error".into(), format!("These ran different suites ({}@{} and {}@{}). Results are only comparable within the same suite version.", sa.suite.0, sa.suite.1, sb.suite.0, sb.suite.1).into());
        return p.render_status(StatusCode::UNPROCESSABLE_ENTITY, "compare.html", &ctx);
    }
    let Some(rs) = state.registry.get(&sa.suite.0, sa.suite.1) else {
        return not_found_page(&p);
    };
    let (a_name, b_name) = if sa.name == sb.name {
        (format!("A · {}", sa.name), format!("B · {}", sb.name))
    } else {
        (sa.name.clone(), sb.name.clone())
    };
    let mut rows = Vec::new();
    let mut table = Vec::new();
    for sc in &rs.manifest.scenarios {
        let (Some(&ta), Some(&tb)) = (sa.medians.get(&sc.id), sb.medians.get(&sc.id)) else {
            continue;
        };
        rows.push(Mirrored {
            label: sc.label(),
            a: ta,
            b: tb,
        });
        let diff = ta as f64 - tb as f64;
        table.push(json!({
            "label": sc.label(), "a": duration_ms(ta), "b": duration_ms(tb),
            "pct": if ta == 0 { "0%".into() } else { format!("{:+.0}%", -diff / ta as f64 * 100.0).replace('-', "\u{2212}") },
            "secs": format!("{}{}", match tb.cmp(&ta) { std::cmp::Ordering::Greater => "+", std::cmp::Ordering::Less => "−", std::cmp::Ordering::Equal => "" }, duration_ms(ta.abs_diff(tb))),
            "b_faster": tb < ta,
        }));
    }
    let headline = rows
        .iter()
        .find(|r| r.label == scenario_label(HEADLINE))
        .or(rows.first());
    let summary = headline.map(|h| phrase(&a_name, &b_name, &h.label, h.a, h.b));
    ctx.insert("title".into(), format!("{} vs {}", sa.name, sb.name).into());
    ctx.insert("a".into(), json!({"name": a_name, "href": sa.href}));
    ctx.insert("b".into(), json!({"name": b_name, "href": sb.href}));
    ctx.insert("suite".into(), suite_json(rs, 0));
    ctx.insert(
        "chart".into(),
        charts::mirrored_bars("Median build time per scenario", &a_name, &b_name, &rows).into(),
    );
    ctx.insert("summary".into(), summary.into());
    ctx.insert("rows".into(), table.into());
    p.render("compare.html", &ctx)
}

async fn insights_memory(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    headers: HeaderMap,
    Query(q): Q,
) -> Response {
    let p = Page {
        state: &state,
        jar: &jar,
        headers: &headers,
    };
    let snap = state.snap();
    let suite = pick_suite(&state, q.get("suite"));
    let m = &suite.manifest;
    let configs = snap.configs(&m.id, m.version, HEADLINE, &Filter::default());
    let mut by_cpu: BTreeMap<(String, String), Vec<&ConfigStats>> = BTreeMap::new();
    for c in &configs {
        by_cpu
            .entry((c.key.cpu_family.clone(), c.key.os_name.clone()))
            .or_default()
            .push(c);
    }
    let panels: Vec<Panel> = by_cpu
        .into_iter()
        .filter(|(_, v)| v.len() >= 2)
        .map(|((cpu, os), mut v)| {
            v.sort_by_key(|c| c.key.mem_gb);
            Panel {
                title: format!("{cpu} · {os}"),
                href: Some(format!(
                    "/machines?suite={}&cpu={}&os={}",
                    enc(&m.key()),
                    enc(&cpu),
                    enc(&os)
                )),
                points: v
                    .iter()
                    .map(|c| (format!("{} GB", c.key.mem_gb), c.median, c.n))
                    .collect(),
            }
        })
        .collect();
    let mut swap: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    for r in snap.rows_for(&m.id, m.version, HEADLINE, &Filter::default()) {
        let e = swap.entry(r.mem_gb).or_default();
        e.0 += 1;
        if r.scenario(HEADLINE).and_then(|s| s.peak_swap).unwrap_or(0) > 256 * 1024 * 1024 {
            e.1 += 1;
        }
    }
    let max_share = swap
        .values()
        .map(|(n, s)| *s as f64 / *n as f64)
        .fold(0.0, f64::max)
        .max(0.01);
    let swap_rows: Vec<_> = swap
        .into_iter()
        .map(|(gb, (n, s))| {
            let share = s as f64 / n as f64;
            json!({"gb": gb, "n": n, "swapped": s, "share": benchpress_core::format::percent(share), "w": format!("{:.1}", share / max_share * 100.0)})
        })
        .collect();
    let mut ctx = p.base(
        "Does more RAM help?",
        &format!(
            "How {} build times change with RAM size on the same CPU.",
            m.project()
        ),
        "/insights/memory",
        "insights",
    );
    ctx.insert("suite".into(), suite_json(suite, 0));
    ctx.insert(
        "suites".into(),
        state
            .registry
            .latest_all()
            .into_iter()
            .map(|s| json!({"key": s.manifest.key(), "label": s.manifest.display_name}))
            .collect::<Vec<_>>()
            .into(),
    );
    ctx.insert(
        "chart".into(),
        (!panels.is_empty())
            .then(|| charts::small_multiples("Median clean release time by RAM size", &panels))
            .into(),
    );
    ctx.insert("swap".into(), swap_rows.into());
    p.render("insights_memory.html", &ctx)
}

async fn insights_cores(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    headers: HeaderMap,
    Query(q): Q,
) -> Response {
    let p = Page {
        state: &state,
        jar: &jar,
        headers: &headers,
    };
    let snap = state.snap();
    let suite = pick_suite(&state, q.get("suite"));
    let m = &suite.manifest;
    let configs = snap.configs(&m.id, m.version, HEADLINE, &Filter::default());
    let vendors = ["Apple", "AMD", "Intel", "Other"];
    let points: Vec<Point> = configs
        .iter()
        .filter_map(|c| {
            Some(Point {
                x: f64::from(c.logical_cores?),
                y_ms: c.median,
                series: c
                    .vendor
                    .as_deref()
                    .and_then(|v| vendors.iter().position(|x| *x == v))
                    .unwrap_or(3),
                label: c.key.label(),
            })
        })
        .collect();
    let used: Vec<&str> = vendors.to_vec();
    let mut ctx = p.base(
        "Do more cores help?",
        &format!(
            "{} clean release time against logical core count.",
            m.project()
        ),
        "/insights/cores",
        "insights",
    );
    ctx.insert("suite".into(), suite_json(suite, 0));
    ctx.insert(
        "suites".into(),
        state
            .registry
            .latest_all()
            .into_iter()
            .map(|s| json!({"key": s.manifest.key(), "label": s.manifest.display_name}))
            .collect::<Vec<_>>()
            .into(),
    );
    ctx.insert(
        "chart".into(),
        (!points.is_empty())
            .then(|| {
                charts::scatter(
                    "Median clean release time by logical cores",
                    "Logical cores",
                    &used,
                    &points,
                )
            })
            .into(),
    );
    p.render("insights_cores.html", &ctx)
}

async fn suites(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Response {
    let p = Page {
        state: &state,
        jar: &jar,
        headers: &headers,
    };
    let snap = state.snap();
    let mut list: Vec<&RegisteredSuite> = state.registry.all().collect();
    list.sort_by_key(|s| (s.manifest.tier, std::cmp::Reverse(s.manifest.version)));
    let suites: Vec<_> = list
        .into_iter()
        .map(|s| {
            suite_json(
                s,
                snap.rows
                    .iter()
                    .filter(|r| {
                        r.suite_id == s.manifest.id && r.suite_version == s.manifest.version
                    })
                    .count(),
            )
        })
        .collect();
    let mut ctx = p.base(
        "Suites",
        "The pinned Rust projects, toolchains and scenarios Benchpress builds.",
        "/suites",
        "suites",
    );
    ctx.insert("suites".into(), suites.into());
    p.render("suites.html", &ctx)
}

async fn about(State(state): State<Arc<AppState>>, jar: CookieJar, headers: HeaderMap) -> Response {
    let p = Page {
        state: &state,
        jar: &jar,
        headers: &headers,
    };
    let mut ctx = p.base(
        "Methodology and privacy",
        "How Benchpress measures builds, what it collects and what it never does.",
        "/about",
        "about",
    );
    ctx.insert(
        "flags".into(),
        [
            benchpress_core::validate::Flag::Battery,
            benchpress_core::validate::Flag::LowPower,
            benchpress_core::validate::Flag::Virtualized,
            benchpress_core::validate::Flag::HighVariance,
            benchpress_core::validate::Flag::Outlier,
        ]
        .iter()
        .map(|f| json!({"id": f.as_str(), "text": f.explain()}))
        .collect::<Vec<_>>()
        .into(),
    );
    p.render("about.html", &ctx)
}

async fn start(State(state): State<Arc<AppState>>, jar: CookieJar, headers: HeaderMap) -> Response {
    let p = Page {
        state: &state,
        jar: &jar,
        headers: &headers,
    };
    let ctx = p.base(
        "Run it now",
        "Run benchpress on Linux or an Apple Silicon Mac without installing it.",
        "/start",
        "start",
    );
    p.render("start.html", &ctx)
}

async fn theme(jar: CookieJar, Query(q): Q) -> (CookieJar, Redirect) {
    let back = q
        .get("back")
        .filter(|b| {
            b.starts_with('/')
                && !b.starts_with("//")
                && !b.contains('\\')
                && !b.chars().any(char::is_control)
        })
        .cloned()
        .unwrap_or_else(|| "/".into());
    let jar = match q.get("set").map(String::as_str) {
        Some(t @ ("light" | "dark")) => jar.add(
            Cookie::build(("theme", t.to_owned()))
                .path("/")
                .max_age(time::Duration::days(365))
                .same_site(SameSite::Lax)
                .http_only(true)
                .build(),
        ),
        _ => jar.remove(Cookie::build("theme").path("/").build()),
    };
    (jar, Redirect::to(&back))
}

fn not_found_page(p: &Page<'_>) -> Response {
    let ctx = p.base("Not found", "That page does not exist.", "", "");
    p.render_status(StatusCode::NOT_FOUND, "404.html", &ctx)
}

/// Fallback for unknown routes.
pub async fn not_found(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Response {
    let p = Page {
        state: &state,
        jar: &jar,
        headers: &headers,
    };
    not_found_page(&p)
}
