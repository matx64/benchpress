//! Minijinja environment with templates embedded at build time.

use minijinja::{Environment, Value};

use crate::assets;

/// Template renderer.
pub struct Views {
    env: Environment<'static>,
}

fn lockup_svg() -> String {
    include_str!("../assets/lockup-dark-bg.svg")
        .replace("<svg ", "<svg class=\"lockup\" ")
        .replace("fill=\"#9AA3AC\"", "fill=\"var(--bp-text-muted)\"")
        .replace("fill=\"#6E757D\"", "fill=\"var(--bp-tier-quick)\"")
        .replace("fill=\"#B4BAC0\"", "fill=\"var(--bp-tier-standard)\"")
        .replace("fill=\"#F1F2EE\"", "fill=\"var(--bp-text)\"")
}

impl Views {
    /// Build the environment.
    pub fn new(public_url: &str) -> Views {
        let mut env = Environment::new();
        minijinja_embed::load_templates!(&mut env);
        env.add_global("public_url", public_url.to_owned());
        env.add_global("lockup", Value::from_safe_string(lockup_svg()));
        env.add_global(
            "sprite",
            Value::from_safe_string(include_str!("../assets/sprite.svg").to_owned()),
        );
        env.add_global("install_cmd", format!("curl -fsSL {public_url}/run | sh"));
        env.add_function("asset", |name: &str| {
            Value::from_safe_string(assets::url(name))
        });
        env.add_filter("duration", |ms: u64| {
            benchpress_core::format::duration_ms(ms)
        });
        env.add_filter("bytes", |b: u64| benchpress_core::format::bytes(b));
        env.add_filter("percent", |f: f64| benchpress_core::format::percent(f));
        env.add_filter("scenario_label", |s: &str| {
            benchpress_core::suite::scenario_label(s)
        });
        Views { env }
    }

    /// Render a template.
    pub fn render(&self, name: &str, ctx: Value) -> Result<String, minijinja::Error> {
        self.env.get_template(name)?.render(ctx)
    }
}
