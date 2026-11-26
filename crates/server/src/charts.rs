//! Server-rendered charts. Every color comes from a `--bp-*` custom property, so charts
//! follow the theme. Each chart carries a title, a description and a hidden data table.

use std::fmt::Write as _;

use benchpress_core::format::duration_ms;

/// Escape text for HTML and SVG.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// A "nice" axis maximum and step for values up to `max`.
pub fn nice_ticks(max: f64, target: usize) -> (f64, f64) {
    if max <= 0.0 {
        return (1.0, 1.0);
    }
    let raw = max / target.max(1) as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 2.5, 5.0, 10.0]
        .iter()
        .map(|m| m * mag)
        .find(|s| *s >= raw)
        .unwrap_or(10.0 * mag);
    ((max / step).ceil() * step, step)
}

/// Nice tick values in milliseconds, aligned to seconds or minutes.
pub fn duration_ticks(max_ms: u64, target: usize) -> (u64, Vec<u64>) {
    let max_s = max_ms as f64 / 1000.0;
    let (top, step) = if max_s > 180.0 {
        let (t, s) = nice_ticks(max_s / 60.0, target);
        (t * 60.0, s * 60.0)
    } else {
        nice_ticks(max_s, target)
    };
    let mut ticks = Vec::new();
    let mut v = 0.0;
    while v <= top + 1e-9 {
        ticks.push((v * 1000.0).round() as u64);
        v += step;
    }
    ((top * 1000.0).round() as u64, ticks)
}

fn sr_table(caption: &str, head: &[&str], rows: &[Vec<String>]) -> String {
    let mut t = format!(
        "<div class=\"sr-only\"><table><caption>{}</caption><thead><tr>",
        esc(caption)
    );
    for h in head {
        let _ = write!(t, "<th scope=\"col\">{}</th>", esc(h));
    }
    t.push_str("</tr></thead><tbody>");
    for r in rows {
        t.push_str("<tr>");
        for c in r {
            let _ = write!(t, "<td>{}</td>", esc(c));
        }
        t.push_str("</tr>");
    }
    t.push_str("</tbody></table></div>");
    t
}

/// One bar of a ranked bar chart.
pub struct Bar {
    pub label: String,
    pub sublabel: String,
    pub value_ms: u64,
    pub href: Option<String>,
    pub note: String,
}

/// Horizontal ranked bars, fastest first, on a shared zero-based scale.
pub fn ranked_bars(title: &str, desc: &str, bars: &[Bar]) -> String {
    let max = bars.iter().map(|b| b.value_ms).max().unwrap_or(1).max(1);
    let mut out = format!(
        "<figure class=\"ranked\" aria-label=\"{}\"><ol class=\"ranked-list\">",
        esc(title)
    );
    for (i, b) in bars.iter().enumerate() {
        let w = (b.value_ms as f64 / max as f64 * 100.0).max(0.5);
        let label = match &b.href {
            Some(h) => format!("<a href=\"{}\">{}</a>", esc(h), esc(&b.label)),
            None => esc(&b.label),
        };
        let _ = write!(
            out,
            "<li class=\"ranked-row\"><span class=\"ranked-rank num\">{}</span><span class=\"ranked-label\">{label}<small>{}</small></span>\
             <svg class=\"ranked-bar\" viewBox=\"0 0 100 12\" preserveAspectRatio=\"none\" aria-hidden=\"true\" focusable=\"false\">\
             <rect x=\"0\" y=\"0\" width=\"{w:.2}\" height=\"12\" fill=\"var(--bp-chart-bar)\"/></svg>\
             <span class=\"ranked-value num\">{}<small>{}</small></span></li>",
            i + 1,
            esc(&b.sublabel),
            duration_ms(b.value_ms),
            esc(&b.note),
        );
    }
    out.push_str("</ol>");
    let rows: Vec<Vec<String>> = bars
        .iter()
        .enumerate()
        .map(|(i, b)| {
            vec![
                (i + 1).to_string(),
                b.label.clone(),
                b.sublabel.clone(),
                duration_ms(b.value_ms),
            ]
        })
        .collect();
    out.push_str(&sr_table(
        &format!("{title}. {desc}"),
        &["Rank", "Configuration", "Details", "Median"],
        &rows,
    ));
    out.push_str("</figure>");
    out
}

/// Inline range bar for a table cell: p25 to p75 with a median tick, on `0..=scale_max`.
pub fn range_bar(p25: u64, median: u64, p75: u64, scale_max: u64) -> String {
    let s = |v: u64| v as f64 / scale_max.max(1) as f64 * 200.0;
    let (a, m, b) = (s(p25), s(median), s(p75));
    format!(
        "<svg class=\"range-bar\" viewBox=\"0 0 200 16\" preserveAspectRatio=\"none\" role=\"img\" aria-label=\"{} to {}, median {}\">\
         <rect x=\"0\" y=\"7.5\" width=\"200\" height=\"1\" fill=\"var(--bp-line)\"/>\
         <rect x=\"{a:.2}\" y=\"4\" width=\"{:.2}\" height=\"8\" rx=\"1\" fill=\"var(--bp-chart-bar)\"/>\
         <rect x=\"{:.2}\" y=\"1\" width=\"2\" height=\"14\" fill=\"var(--bp-text)\"/></svg>",
        duration_ms(p25),
        duration_ms(p75),
        duration_ms(median),
        (b - a).max(1.0),
        (m - 1.0).clamp(0.0, 198.0),
    )
}

/// Distribution strip with one highlighted value that slides into place.
pub fn dot_strip(title: &str, values: &[u64], highlight: Option<u64>) -> String {
    const W: f64 = 640.0;
    const PAD: f64 = 12.0;
    let all_max = values.iter().copied().chain(highlight).max().unwrap_or(1);
    let (top, ticks) = duration_ticks(all_max, 4);
    let x = |v: u64| PAD + v as f64 / top.max(1) as f64 * (W - 2.0 * PAD);
    let mut svg = format!(
        "<svg class=\"dot-strip\" viewBox=\"0 0 {W} 72\" role=\"img\" aria-labelledby=\"ds-t ds-d\"><title id=\"ds-t\">{}</title>\
         <desc id=\"ds-d\">{} results{}</desc>",
        esc(title),
        values.len(),
        highlight
            .map(|h| format!("; this result is {}", duration_ms(h)))
            .unwrap_or_default()
    );
    let _ = write!(
        svg,
        "<rect x=\"{PAD}\" y=\"35.5\" width=\"{}\" height=\"1\" fill=\"var(--bp-line)\"/>",
        W - 2.0 * PAD
    );
    for t in &ticks {
        let tx = x(*t);
        let anchor = if *t == 0 {
            "start"
        } else if *t == top {
            "end"
        } else {
            "middle"
        };
        let _ = write!(
            svg,
            "<rect x=\"{tx:.1}\" y=\"52\" width=\"1\" height=\"4\" fill=\"var(--bp-line)\"/><text x=\"{tx:.1}\" y=\"70\" text-anchor=\"{anchor}\" class=\"tick\">{}</text>",
            if *t == 0 {
                "0".to_owned()
            } else {
                duration_ms(*t)
            }
        );
    }
    let mut sorted: Vec<u64> = values.to_vec();
    sorted.sort_unstable();
    let mut placed: Vec<(f64, f64)> = Vec::new();
    for v in &sorted {
        let cx = x(*v);
        let mut cy = 36.0;
        for k in 0..12 {
            let off = f64::from((k + 1) / 2) * 7.0 * if k % 2 == 0 { 1.0 } else { -1.0 };
            let cand = 36.0 + if k == 0 { 0.0 } else { off };
            if placed
                .iter()
                .all(|(px, py)| (px - cx).powi(2) + (py - cand).powi(2) >= 49.0)
            {
                cy = cand;
                break;
            }
        }
        placed.push((cx, cy));
        let _ = write!(
            svg,
            "<circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"3.5\" fill=\"var(--bp-chart-bar)\"/>"
        );
    }
    if let Some(h) = highlight {
        let hx = x(h);
        let _ = write!(
            svg,
            "<circle class=\"you-dot\" style=\"--from:{:.1}px\" cx=\"{hx:.1}\" cy=\"36\" r=\"8\" fill=\"var(--bp-you)\" stroke=\"var(--bp-you-outline)\" stroke-width=\"2\"/>",
            PAD - hx
        );
    }
    svg.push_str("</svg>");
    let rows: Vec<Vec<String>> = sorted.iter().map(|v| vec![duration_ms(*v)]).collect();
    format!(
        "<figure class=\"chart\">{svg}{}</figure>",
        sr_table(title, &["Median"], &rows)
    )
}

/// One row of a comparison.
pub struct Mirrored {
    pub label: String,
    pub a: u64,
    pub b: u64,
}

/// Mirrored bars: A grows left, B grows right, from a shared centre axis.
pub fn mirrored_bars(title: &str, a_name: &str, b_name: &str, rows: &[Mirrored]) -> String {
    let max = rows
        .iter()
        .flat_map(|r| [r.a, r.b])
        .max()
        .unwrap_or(1)
        .max(1);
    let mut out = format!(
        "<figure class=\"mirrored\" aria-label=\"{}\"><div class=\"mirrored-head\"><span>{}</span><span>{}</span></div><ol class=\"mirrored-list\">",
        esc(title),
        esc(a_name),
        esc(b_name)
    );
    for r in rows {
        let wa = r.a as f64 / max as f64 * 100.0;
        let wb = r.b as f64 / max as f64 * 100.0;
        let _ = write!(
            out,
            "<li class=\"mirrored-row\"><span class=\"mirrored-label\">{}</span>\
             <span class=\"mirrored-side a\"><span class=\"num\">{}</span><svg viewBox=\"0 0 100 12\" preserveAspectRatio=\"none\" aria-hidden=\"true\"><rect x=\"{:.2}\" width=\"{wa:.2}\" height=\"12\" fill=\"var(--bp-chart-bar)\"/></svg></span>\
             <span class=\"mirrored-side b\"><svg viewBox=\"0 0 100 12\" preserveAspectRatio=\"none\" aria-hidden=\"true\"><rect width=\"{wb:.2}\" height=\"12\" fill=\"var(--bp-tier-standard)\"/></svg><span class=\"num\">{}</span></span></li>",
            esc(&r.label),
            duration_ms(r.a),
            100.0 - wa,
            duration_ms(r.b),
        );
    }
    out.push_str("</ol>");
    let data: Vec<Vec<String>> = rows
        .iter()
        .map(|r| vec![r.label.clone(), duration_ms(r.a), duration_ms(r.b)])
        .collect();
    out.push_str(&sr_table(title, &["Scenario", a_name, b_name], &data));
    out.push_str("</figure>");
    out
}

/// One panel of small multiples.
pub struct Panel {
    pub title: String,
    pub href: Option<String>,
    pub points: Vec<(String, u64, usize)>,
}

/// Small column charts with a shared y scale, one per panel.
pub fn small_multiples(title: &str, panels: &[Panel]) -> String {
    let max = panels
        .iter()
        .flat_map(|p| p.points.iter().map(|x| x.1))
        .max()
        .unwrap_or(1)
        .max(1);
    let (top, _) = duration_ticks(max, 3);
    let mut out = format!(
        "<div class=\"multiples\" role=\"list\" aria-label=\"{}\">",
        esc(title)
    );
    let mut data = Vec::new();
    for p in panels {
        let n = p.points.len().max(1);
        let col_w = 200.0 / n as f64;
        let mut svg = format!(
            "<svg viewBox=\"0 0 200 140\" role=\"img\" aria-label=\"{}\"><rect x=\"0\" y=\"110.5\" width=\"200\" height=\"1\" fill=\"var(--bp-line)\"/>",
            esc(&p.title)
        );
        for (i, (label, v, count)) in p.points.iter().enumerate() {
            let h = *v as f64 / top.max(1) as f64 * 88.0;
            let x = i as f64 * col_w + col_w * 0.2;
            let w = col_w * 0.6;
            let _ = write!(
                svg,
                "<rect x=\"{x:.1}\" y=\"{:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" rx=\"2\" fill=\"var(--bp-chart-bar)\"/>\
                 <text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" class=\"value\">{}</text>\
                 <text x=\"{:.1}\" y=\"128\" text-anchor=\"middle\" class=\"tick\">{}</text>",
                110.0 - h,
                x + w / 2.0,
                104.0 - h,
                duration_ms(*v),
                x + w / 2.0,
                esc(label),
            );
            data.push(vec![
                p.title.clone(),
                label.clone(),
                duration_ms(*v),
                count.to_string(),
            ]);
        }
        svg.push_str("</svg>");
        let heading = match &p.href {
            Some(h) => format!("<a href=\"{}\">{}</a>", esc(h), esc(&p.title)),
            None => esc(&p.title),
        };
        let _ = write!(
            out,
            "<figure class=\"multiple\" role=\"listitem\"><figcaption>{heading}</figcaption>{svg}</figure>"
        );
    }
    out.push_str("</div>");
    out.push_str(&sr_table(
        title,
        &["CPU", "RAM", "Median", "Results"],
        &data,
    ));
    out
}

/// One scatter point.
pub struct Point {
    pub x: f64,
    pub y_ms: u64,
    pub series: usize,
    pub label: String,
}

/// Scatter of time against a numeric x. Series are told apart by marker shape and lightness, not hue.
pub fn scatter(title: &str, x_label: &str, series: &[&str], points: &[Point]) -> String {
    const W: f64 = 640.0;
    const H: f64 = 360.0;
    let (left, right, top, bottom) = (56.0, 16.0, 16.0, 44.0);
    let x_max = points.iter().map(|p| p.x).fold(0.0, f64::max);
    let (x_top, x_step) = nice_ticks(x_max.max(1.0), 6);
    let (y_top, y_ticks) = duration_ticks(points.iter().map(|p| p.y_ms).max().unwrap_or(1), 4);
    let sx = |v: f64| left + v / x_top * (W - left - right);
    let sy = |v: u64| H - bottom - v as f64 / y_top.max(1) as f64 * (H - top - bottom);
    let mut svg = format!(
        "<svg class=\"scatter\" viewBox=\"0 0 {W} {H}\" role=\"img\" aria-labelledby=\"sc-t\"><title id=\"sc-t\">{}</title><desc>{} results</desc>",
        esc(title),
        points.len()
    );
    for v in &y_ticks {
        let y = sy(*v);
        let _ = write!(
            svg,
            "<rect x=\"{left}\" y=\"{y:.1}\" width=\"{}\" height=\"1\" fill=\"var(--bp-chart-grid)\"/><text x=\"{}\" y=\"{:.1}\" text-anchor=\"end\" class=\"tick\">{}</text>",
            W - left - right,
            left - 8.0,
            y + 4.0,
            if *v == 0 { "0".into() } else { duration_ms(*v) }
        );
    }
    let mut xv = 0.0;
    while xv <= x_top + 1e-9 {
        let _ = write!(
            svg,
            "<text x=\"{:.1}\" y=\"{}\" text-anchor=\"middle\" class=\"tick\">{xv}</text>",
            sx(xv),
            H - bottom + 18.0
        );
        xv += x_step;
    }
    let _ = write!(
        svg,
        "<text x=\"{:.1}\" y=\"{}\" text-anchor=\"middle\" class=\"axis\">{}</text>",
        left + (W - left - right) / 2.0,
        H - 4.0,
        esc(x_label)
    );
    let fills = [
        "var(--bp-tier-heavy)",
        "var(--bp-tier-standard)",
        "var(--bp-tier-quick)",
        "var(--bp-text-muted)",
    ];
    for p in points {
        let (cx, cy) = (sx(p.x), sy(p.y_ms));
        let fill = fills[p.series % fills.len()];
        let shape = marker(p.series, cx, cy, fill);
        let _ = write!(
            svg,
            "<g><title>{}: {}</title>{shape}</g>",
            esc(&p.label),
            duration_ms(p.y_ms)
        );
    }
    svg.push_str("</svg>");
    let mut legend = String::from("<ul class=\"legend\">");
    for (i, s) in series.iter().enumerate() {
        let _ = write!(
            legend,
            "<li><svg viewBox=\"0 0 16 16\" aria-hidden=\"true\">{}</svg>{}</li>",
            marker(i, 8.0, 8.0, fills[i % fills.len()]),
            esc(s)
        );
    }
    legend.push_str("</ul>");
    let data: Vec<Vec<String>> = points
        .iter()
        .map(|p| {
            vec![
                p.label.clone(),
                series.get(p.series).copied().unwrap_or("").to_owned(),
                p.x.to_string(),
                duration_ms(p.y_ms),
            ]
        })
        .collect();
    format!(
        "<figure class=\"chart\"><div class=\"chart-scroll\">{svg}</div>{legend}{}</figure>",
        sr_table(
            title,
            &["Configuration", "Vendor", x_label, "Median"],
            &data
        )
    )
}

fn marker(series: usize, cx: f64, cy: f64, fill: &str) -> String {
    match series % 4 {
        0 => format!("<circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"5\" fill=\"{fill}\"/>"),
        1 => format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"9\" height=\"9\" fill=\"{fill}\"/>",
            cx - 4.5,
            cy - 4.5
        ),
        2 => format!(
            "<path d=\"M{cx:.1} {:.1}L{:.1} {:.1}L{:.1} {:.1}Z\" fill=\"{fill}\"/>",
            cy - 6.0,
            cx + 5.5,
            cy + 4.0,
            cx - 5.5,
            cy + 4.0
        ),
        _ => format!(
            "<path d=\"M{cx:.1} {:.1}L{:.1} {cy:.1}L{cx:.1} {:.1}L{:.1} {cy:.1}Z\" fill=\"{fill}\"/>",
            cy - 6.0,
            cx + 6.0,
            cy + 6.0,
            cx - 6.0
        ),
    }
}
