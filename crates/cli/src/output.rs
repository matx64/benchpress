//! All terminal output goes through this module. Colors follow the brand terminal palette.

use std::io::{IsTerminal, Write};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use benchpress_core::Tier;

static JSON_MODE: AtomicBool = AtomicBool::new(false);
static MENU_MODE: AtomicBool = AtomicBool::new(false);
static DEPTH: OnceLock<Depth> = OnceLock::new();

#[derive(Clone, Copy, PartialEq, Eq)]
enum Depth {
    None,
    Basic,
    Ansi256,
    True,
}

/// Brand roles from `BRAND.md`'s terminal table.
#[derive(Clone, Copy)]
pub enum Role {
    Muted,
    Quick,
    Standard,
    Heavy,
    You,
    Success,
    Error,
    Warning,
    Strong,
}

impl Role {
    fn rgb(self) -> (u8, u8, u8) {
        match self {
            Role::Muted => (0x9A, 0xA3, 0xAC),
            Role::Quick => (0x6E, 0x75, 0x7D),
            Role::Standard => (0xB4, 0xBA, 0xC0),
            Role::Heavy | Role::Strong => (0xF1, 0xF2, 0xEE),
            Role::You => (0xF5, 0xE6, 0x63),
            Role::Success => (0x8E, 0xD1, 0xA5),
            Role::Error => (0xF2, 0x87, 0x7E),
            Role::Warning => (0xF5, 0xB9, 0x71),
        }
    }

    fn ansi256(self) -> u8 {
        match self {
            Role::Muted => 247,
            Role::Quick => 243,
            Role::Standard => 250,
            Role::Heavy | Role::Strong => 255,
            Role::You => 221,
            Role::Success => 115,
            Role::Error => 210,
            Role::Warning => 215,
        }
    }

    fn basic(self) -> &'static str {
        match self {
            Role::Muted | Role::Quick => "90",
            Role::Standard => "37",
            Role::Heavy => "97",
            Role::You | Role::Warning => "33",
            Role::Success => "32",
            Role::Error => "31",
            Role::Strong => "1",
        }
    }
}

fn depth() -> Depth {
    *DEPTH.get_or_init(|| {
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        let tty = std::io::stderr().is_terminal() && std::io::stdout().is_terminal();
        if no_color || !tty || std::env::var("TERM").is_ok_and(|t| t == "dumb") {
            return Depth::None;
        }
        match std::env::var("COLORTERM").as_deref() {
            Ok("truecolor" | "24bit") => Depth::True,
            _ if std::env::var("TERM").is_ok_and(|t| t.contains("256")) => Depth::Ansi256,
            _ => Depth::Basic,
        }
    })
}

/// Whether styled, interactive output is in use.
pub fn is_fancy() -> bool {
    depth() != Depth::None && !JSON_MODE.load(Ordering::Relaxed)
}

/// Whether stdin and stderr are interactive.
pub fn is_interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

/// Wrap text in the role's color.
pub fn paint(role: Role, text: &str) -> String {
    let code = match depth() {
        Depth::None => return text.to_owned(),
        Depth::True if !matches!(role, Role::Strong) => {
            let (r, g, b) = role.rgb();
            format!("38;2;{r};{g};{b}")
        }
        Depth::Ansi256 if !matches!(role, Role::Strong) => format!("38;5;{}", role.ansi256()),
        _ => role.basic().to_owned(),
    };
    format!("\x1b[{code}m{text}\x1b[0m")
}

/// Tier plate glyph in the tier's color.
pub fn plate(tier: Tier) -> String {
    let role = match tier {
        Tier::Quick => Role::Quick,
        Tier::Standard => Role::Standard,
        Tier::Heavy => Role::Heavy,
    };
    paint(role, "▮")
}

/// `--json` sends human output to stderr so stdout carries only JSON.
pub fn set_json_mode(on: bool) {
    JSON_MODE.store(on, Ordering::Relaxed);
}

/// Print one human-readable line.
pub fn line(text: impl AsRef<str>) {
    let text = text.as_ref();
    if JSON_MODE.load(Ordering::Relaxed) {
        eprintln!("{text}");
    } else {
        println!("{text}");
    }
}

/// Print an empty line.
pub fn blank() {
    line("");
}

/// Print machine output to stdout.
pub fn data(text: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{text}");
}

/// Print a warning.
pub fn warn(text: impl AsRef<str>) {
    eprintln!("{} {}", paint(Role::Warning, "warning:"), text.as_ref());
}

/// Print an error.
pub fn error(text: impl AsRef<str>) {
    eprintln!("{} {}", paint(Role::Error, "error:"), text.as_ref());
}

/// Print a hint.
pub fn hint(text: impl AsRef<str>) {
    line(format!("{} {}", paint(Role::You, "→"), text.as_ref()));
}

/// Label: value, with a muted label.
pub fn field(label: &str, value: impl AsRef<str>) {
    line(format!(
        "  {} {}",
        paint(Role::Muted, &format!("{label:<12}")),
        value.as_ref()
    ));
}

/// Pick from a menu. `None` when the user backs out with Esc, q or Ctrl+C.
pub fn choose<T: std::fmt::Display>(
    prompt: &str,
    items: &[T],
    default: usize,
) -> anyhow::Result<Option<usize>> {
    let picked = dialoguer::Select::new()
        .with_prompt(prompt)
        .items(items)
        .default(default)
        .interact_opt();
    let _ = console::Term::stderr().show_cursor();
    match picked {
        Ok(Some(p)) => {
            crate::interrupt::reset();
            Ok(Some(p))
        }
        Ok(None) => Ok(None),
        Err(dialoguer::Error::IO(e)) if e.kind() == std::io::ErrorKind::Interrupted => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Inside the menu.
pub fn set_menu_mode(on: bool) {
    MENU_MODE.store(on, Ordering::Relaxed);
}

/// Point the user at a later action: the menu when launched by `run.sh`, a command otherwise.
pub fn later(command: &str, menu: &str) -> String {
    if MENU_MODE.load(Ordering::Relaxed) || crate::paths::ephemeral() {
        format!("from {menu}")
    } else {
        format!("with `benchpress {command}`")
    }
}

/// Ask a yes/no question on stderr. Non-interactive sessions get `default`.
pub fn confirm(prompt: &str, default: bool) -> bool {
    if !is_interactive() {
        return default;
    }
    dialoguer::Confirm::new()
        .with_prompt(prompt)
        .default(default)
        .interact()
        .inspect(|_| crate::interrupt::reset())
        .unwrap_or_else(|_| {
            let _ = console::Term::stderr().show_cursor();
            false
        })
}

/// Show long text through `$PAGER` (or `less`), falling back to printing.
pub fn page(text: &str) {
    if is_interactive() && text.lines().count() > 40 {
        let pager = std::env::var("PAGER").unwrap_or_else(|_| "less -R".into());
        let mut parts = pager.split_whitespace();
        if let Some(bin) = parts.next()
            && let Ok(mut child) = std::process::Command::new(bin)
                .args(parts)
                .stdin(std::process::Stdio::piped())
                .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
            return;
        }
    }
    line(text);
}
