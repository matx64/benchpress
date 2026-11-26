//! Open Graph share cards: the brand kit's result template filled in and rendered with resvg.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};

use anyhow::{Context, Result};
use axum::body::Bytes;
use benchpress_core::Registry;
use benchpress_core::Tier;
use benchpress_core::format::duration_ms;
use benchpress_core::hardware::{cpu_family, memory_bucket};
use benchpress_core::suite::scenario_label;

use crate::charts::esc;
use crate::db::Stored;
use crate::stats::{Filter, Snapshot};

const TEMPLATE: &str = include_str!("../assets/og-result-template.svg");

static FONTS: LazyLock<(Arc<resvg::usvg::fontdb::Database>, String)> = LazyLock::new(|| {
    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_font_data(include_bytes!("../assets/Archivo-Variable.ttf").to_vec());
    let body = db
        .faces()
        .next()
        .and_then(|f| f.families.first())
        .map_or_else(|| "Archivo".to_owned(), |(name, _)| name.clone());
    db.load_font_data(include_bytes!("../assets/BenchpressDisplay-Regular.ttf").to_vec());
    db.set_sans_serif_family(&body);
    (Arc::new(db), body)
});

/// Memory cache backed by a directory on disk.
pub struct Cache {
    dir: PathBuf,
    mem: Mutex<HashMap<String, Bytes>>,
}

impl Cache {
    /// Cache rooted at `dir` (created lazily).
    pub fn new(dir: PathBuf) -> Cache {
        Cache {
            dir,
            mem: Mutex::new(HashMap::new()),
        }
    }

    fn path(&self, id: &str) -> Option<PathBuf> {
        id.chars()
            .all(|c| c.is_ascii_alphanumeric())
            .then(|| self.dir.join(format!("{id}.png")))
    }

    /// Cached PNG for `id`.
    pub fn get(&self, id: &str) -> Option<Bytes> {
        if let Some(b) = self.mem.lock().ok()?.get(id) {
            return Some(b.clone());
        }
        let bytes = Bytes::from(std::fs::read(self.path(id)?).ok()?);
        self.mem.lock().ok()?.insert(id.to_owned(), bytes.clone());
        Some(bytes)
    }

    /// Store a PNG for `id`.
    pub fn put(&self, id: &str, png: Vec<u8>) -> Bytes {
        let bytes = Bytes::from(png);
        if let Some(p) = self.path(id)
            && std::fs::create_dir_all(&self.dir).is_ok()
        {
            let _ = std::fs::write(p, &bytes);
        }
        if let Ok(mut m) = self.mem.lock() {
            m.insert(id.to_owned(), bytes.clone());
        }
        bytes
    }

    /// Drop a card, e.g. after the result is unpublished.
    pub fn forget(&self, id: &str) {
        if let Ok(mut m) = self.mem.lock() {
            m.remove(id);
        }
        if let Some(p) = self.path(id) {
            let _ = std::fs::remove_file(p);
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}

/// Render the 1200×630 PNG card for a stored result.
pub fn render(registry: &Registry, snap: &Snapshot, stored: &Stored) -> Result<Vec<u8>> {
    let r = &stored.result;
    let suite = registry
        .get(&r.suite.id, r.suite.version)
        .context("unknown suite")?;
    let m = &suite.manifest;
    let headline = r
        .scenario("clean-release")
        .or(r.scenarios.first())
        .context("no scenarios")?;
    let this = headline.summary.median_wall_ms;
    let others: Vec<u64> = snap
        .rows_for(&m.id, m.version, &headline.id, &Filter::default())
        .filter(|row| row.id != stored.id)
        .filter_map(|row| row.scenario(&headline.id).map(|s| s.median_wall_ms))
        .collect();
    let (tier_color, h) = match m.tier {
        Tier::Quick => ("#6E757D", 16),
        Tier::Standard => ("#B4BAC0", 24),
        Tier::Heavy => ("#F1F2EE", 32),
    };
    let machine = [
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
    .join(", ");
    let max = others
        .iter()
        .copied()
        .chain([this])
        .max()
        .unwrap_or(1)
        .max(1) as f64
        * 1.05;
    let x = |v: u64| 80.0 + v as f64 / max * 640.0;
    let mut dots = String::new();
    for v in &others {
        let _ = write!(
            dots,
            "<circle cx=\"{:.1}\" cy=\"520\" r=\"5\" fill=\"#6E757D\"/>",
            x(*v)
        );
    }
    let _ = write!(
        dots,
        "<circle cx=\"{:.1}\" cy=\"520\" r=\"9\" fill=\"#F5E663\"/>",
        x(this)
    );
    let comparison = if others.is_empty() {
        "The first result for this suite.".to_owned()
    } else {
        let slower = others.iter().filter(|v| **v > this).count();
        format!("Faster than {slower} of {} other results.", others.len())
    };
    let comment_end = TEMPLATE.find("-->").map_or(0, |i| i + 3);
    let comment_start = TEMPLATE.find("<!--").unwrap_or(comment_end);
    let template = format!("{}{}", &TEMPLATE[..comment_start], &TEMPLATE[comment_end..]);
    let (fonts, body) = &*FONTS;
    let svg = template
        .replacen(
            "font-family=\"Archivo\"",
            &format!("font-family=\"{body}\""),
            1,
        )
        .replace("{{tier_label}}", m.tier.label())
        .replace("{{tier_color}}", tier_color)
        .replace("{{tier_plate_height}}", &h.to_string())
        .replace("{{tier_plate_y}}", &(99 - h).to_string())
        .replace(
            "{{scenario}}",
            &esc(&format!("{} build", scenario_label(&headline.id))),
        )
        .replace("{{time}}", &esc(&duration_ms(this)))
        .replace("{{machine}}", &esc(&truncate(&machine, 48)))
        .replace(
            "{{suite}}",
            &esc(&format!(
                "{} v{}, {}, median of {} runs",
                m.id,
                m.version,
                m.project(),
                headline.runs.len()
            )),
        )
        .replace("{{comparison}}", &esc(&comparison))
        .replace("{{distribution}}", &dots);
    let opts = resvg::usvg::Options {
        fontdb: fonts.clone(),
        ..resvg::usvg::Options::default()
    };
    let tree = resvg::usvg::Tree::from_str(&svg, &opts).context("parsing card svg")?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(1200, 630).context("allocating pixmap")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().context("encoding png")
}
