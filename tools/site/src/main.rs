//! AutoUpload website generator (Rust port of the former tools/build_site.py).
//!
//! Builds the public site (GitHub Pages) from the fragments in docs/_src/<lang>/:
//!   docs/*.html      English pages
//!   docs/es/*.html   Spanish pages
//!
//! Run it from any directory inside the repository:
//!   cargo run -p autoupload-site
//!
//! Each fragment starts with an HTML comment of `key: value` lines (title, h1, callno, desc,
//! lede, rail). `<h2 id="...">` headings build the contents list, `{PERMS}` inserts
//! _src/<lang>/_perms.html, and `{REPO}`, `{ZIP}`, `{EMAIL}` are replaced with the links below.
//! To add a page, add its name to PAGES and its fragment to docs/_src/en/ and docs/_src/es/.

use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Site settings
// ---------------------------------------------------------------------------

/// Public repository of the app (change here if the GitHub account or repository name differs).
const GITHUB_OWNER: &str = "DaoSource111";
const GITHUB_REPO: &str = "autoupload";
const EMAIL: &str = "ellipsio.studio@gmail.com";

/// Pages built for each language, by file name without `.html`. The nav labels come from `Ui::nav`.
const PAGES: [&str; 6] = ["index", "install", "agents", "privacy", "terms", "support"];

/// Interface strings for one language.
struct Ui {
    /// `lang` attribute and output folder key ("en" goes to docs/, "es" to docs/es/).
    code: &'static str,
    /// "Last updated" line shown under the page heading.
    updated: &'static str,
    /// Labels for PAGES, in the same order.
    nav: [&'static str; 6],
    skip: &'static str,
    contents: &'static str,
    /// Text and link label of the language switch (points to the other language).
    other: &'static str,
    other_label: &'static str,
    by: &'static str,
    to_dark: &'static str,
    to_light: &'static str,
    source: &'static str,
    foot: &'static str,
}

const UI_EN: Ui = Ui {
    code: "en",
    updated: "Last updated: November 25, 2026",
    nav: ["Home", "Install", "AI agents", "Privacy", "Terms", "Support"],
    skip: "Skip to content",
    contents: "Contents",
    other: "ES",
    other_label: "Leer en español",
    by: "by <em>Ellipsio Studio</em>",
    to_dark: "Switch to night view",
    to_light: "Switch to light view",
    source: "Source code on GitHub",
    foot: "AutoUpload is a free tool by Ellipsio Studio, Peru. It is not affiliated with, endorsed or \
           sponsored by Google LLC or YouTube. This site sets no cookies and loads nothing from third parties.",
};

const UI_ES: Ui = Ui {
    code: "es",
    updated: "Última actualización: 25 de noviembre de 2026",
    nav: ["Inicio", "Instalación", "Agentes IA", "Privacidad", "Términos", "Soporte"],
    skip: "Ir al contenido",
    contents: "Contenido",
    other: "EN",
    other_label: "Read in English",
    by: "de <em>Ellipsio Studio</em>",
    to_dark: "Cambiar a vista noche",
    to_light: "Cambiar a vista clara",
    source: "Código fuente en GitHub",
    foot: "AutoUpload es una herramienta gratuita de Ellipsio Studio, Perú. No está afiliada, respaldada ni \
           patrocinada por Google LLC ni YouTube. Este sitio no usa cookies ni carga nada de terceros.",
};

/// Languages to build, in output order.
const LANGS: [&Ui; 2] = [&UI_EN, &UI_ES];

// AutoUpload mark: a cobalt tile holding an "A" drawn as an upward stroke; its crossbar is the mint
// time slot the video lands in. Deliberately unrelated to the Source Grade ring monogram.
const SEAL: &str = concat!(
    r##"<svg viewBox="0 0 100 100" aria-hidden="true">"##,
    r##"<rect x="3" y="3" width="94" height="94" rx="24" fill="#1f3bff"/>"##,
    r##"<rect x="3" y="3" width="94" height="94" rx="24" fill="none" stroke="#06080c" stroke-opacity=".25" stroke-width="2"/>"##,
    r##"<path d="M27 76 L50 25 L73 76" fill="none" stroke="#ffffff" stroke-width="10" stroke-linecap="round" stroke-linejoin="round"/>"##,
    r##"<path d="M34 59 H68" fill="none" stroke="#5dfbd3" stroke-width="9" stroke-linecap="round"/>"##,
    r##"<circle cx="80" cy="59" r="4.5" fill="#5dfbd3"/>"##,
    r##"</svg>"##,
);
const GH: &str = r##"<svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8z"/></svg>"##;
const SUN: &str = concat!(
    r##"<svg class="sun" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">"##,
    r##"<circle cx="12" cy="12" r="4.5"/><path d="M12 1.5v2.5M12 20v2.5M1.5 12H4M20 12h2.5M4.6 4.6l1.8 1.8M17.6 17.6l1.8 1.8M4.6 19.4l1.8-1.8M17.6 6.4l1.8-1.8"/></svg>"##,
);
const MOON: &str = concat!(
    r##"<svg class="moon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" aria-hidden="true">"##,
    r##"<path d="M20.5 14.5A8.5 8.5 0 0 1 9.5 3.5a8.5 8.5 0 1 0 11 11z"/></svg>"##,
);
// Applied before first paint so a saved choice never flashes the other theme.
const THEME_BOOT: &str = concat!(
    r##"<script>try{var t=localStorage.getItem("au-theme");if(t==="dark"||t==="light")"##,
    r##"document.documentElement.setAttribute("data-theme",t)}catch(e){}</script>"##,
);

// ---------------------------------------------------------------------------
// Generator
// ---------------------------------------------------------------------------

/// Paths and links for one build.
struct Site {
    root: PathBuf,
    /// Output folder: docs/ in the public repository (web/ in older working copies).
    out: PathBuf,
    /// Fragment folder, out/_src, with one sub-folder per language.
    src: PathBuf,
    repo: String,
    zip: String,
}

impl Site {
    /// Locates the repository root from this crate's manifest directory (tools/site, two levels down).
    fn locate() -> Self {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = manifest
            .parent()
            .and_then(Path::parent)
            .expect("tools/site must sit two levels below the repository root")
            .to_path_buf();
        let out = if root.join("docs").join("_src").exists() {
            root.join("docs")
        } else {
            root.join("web")
        };
        let src = out.join("_src");
        let repo = format!("https://github.com/{GITHUB_OWNER}/{GITHUB_REPO}");
        let zip = format!("{repo}/releases/latest"); // the download button: every system's package
        Site {
            root,
            out,
            src,
            repo,
            zip,
        }
    }

    /// Path shown in the "wrote" messages, relative to the repository root with forward slashes.
    fn display(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }

    /// Reads a UTF-8 text file with Python-style universal newlines (CRLF and CR become LF).
    fn read_text(path: &Path) -> String {
        let raw = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        raw.replace("\r\n", "\n").replace('\r', "\n")
    }

    /// Splits a fragment into its header metadata and its body, with the placeholders filled in.
    fn parse(&self, path: &Path) -> (HashMap<String, String>, String) {
        let text = Self::read_text(path);
        let header = Regex::new(r"(?s)^<!--(.*?)-->\s*").expect("header pattern is valid");
        let m = header.captures(&text).unwrap_or_else(|| {
            panic!(
                "{} must start with a <!-- key: value --> header",
                path.display()
            )
        });
        let mut meta = HashMap::new();
        for line in m[1].trim().lines() {
            let (k, v) = line.split_once(':').unwrap_or((line, ""));
            meta.insert(k.trim().to_string(), v.trim().to_string());
        }
        let mut body = text[m.get(0).expect("whole match").end()..].to_string();
        if body.contains("{PERMS}") {
            // Shared permissions block, one per language.
            let perms_path = path
                .parent()
                .expect("fragment has a folder")
                .join("_perms.html");
            let perms = Self::read_text(&perms_path);
            body = body.replace("{PERMS}", perms.trim());
        }
        body = body
            .replace("{REPO}", &self.repo)
            .replace("{ZIP}", &self.zip)
            .replace("{EMAIL}", EMAIL);
        (meta, body)
    }

    /// Short content hash so browsers fetch a changed file instead of a cached one.
    fn ver(&self, name: &str) -> String {
        let path = self.out.join(name);
        let mut bytes =
            fs::read(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        // Hash the text with LF line ends, so a Windows checkout (CRLF) and CI (LF) agree.
        bytes.retain(|&b| b != b'\r');
        Sha256::digest(&bytes)
            .iter()
            .take(4)
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// Builds the full HTML document for one page in one language.
    fn page(&self, ui: &Ui, name: &str) -> String {
        let lang = ui.code;
        let path = self.src.join(lang).join(format!("{name}.html"));
        let (meta, body) = self.parse(&path);
        let required = |key: &str| -> &str {
            meta.get(key)
                .map(String::as_str)
                .unwrap_or_else(|| panic!("{} is missing the '{key}:' header line", path.display()))
        };
        let title = required("title");
        let desc = html_escape(required("desc"));
        let callno = required("callno");
        let lede = required("lede");
        let h1 = match meta.get("h1") {
            Some(h) if !h.is_empty() => h.as_str(),
            _ => title,
        };

        let pre = if lang == "es" { "../" } else { "" };
        let other_lang = if lang == "en" { "es" } else { "en" };
        let other_href = format!("{}{name}.html", if lang == "en" { "es/" } else { "../" });
        let alt_en = format!("{}{name}.html", if lang == "en" { "" } else { "../" });
        let alt_es = format!("{}{name}.html", if lang == "en" { "es/" } else { "" });
        let contents = ui.contents;
        let repo = self.repo.as_str();
        let other_text = ui.other;
        let other_label = ui.other_label;
        let to_dark = ui.to_dark;
        let to_light = ui.to_light;
        let source = ui.source;
        let by = ui.by;
        let skip = ui.skip;
        let foot = ui.foot;
        let updated = ui.updated;
        let style_v = self.ver("style.css");
        let site_v = self.ver("site.js");

        let tabs: String = PAGES
            .iter()
            .zip(ui.nav.iter())
            .map(|(p, label)| {
                let current = if *p == name {
                    " aria-current=\"page\""
                } else {
                    ""
                };
                format!(r##"<a href="{p}.html"{current}>{label}</a>"##)
            })
            .collect();
        let tools = format!(
            r##"<a href="{repo}" rel="noopener" aria-label="{source}" title="{source}">{GH}</a><a href="{other_href}" hreflang="{other_lang}" lang="{other_lang}" data-lang="{other_lang}" aria-label="{other_label}" title="{other_label}">{other_text}</a><button class="theme" type="button" data-to-dark="{to_dark}" data-to-light="{to_light}" aria-label="{to_dark}">{SUN}{MOON}</button>"##
        );

        // The contents list needs a rail (meta `rail: yes`) and at least one <h2 id>.
        let items = toc_items(&body);
        let rail = meta.get("rail").map(String::as_str) == Some("yes") && !items.is_empty();
        let toc_list: String = items
            .iter()
            .map(|(h, n, l)| {
                format!(
                    r##"<li><a href="#{h}"><span>{}</span>{}</a></li>"##,
                    html_escape(n),
                    html_escape(l)
                )
            })
            .collect();
        let toc_inline: String = items
            .iter()
            .map(|(h, _, l)| format!(r##"<li><a href="#{h}">{}</a></li>"##, html_escape(l)))
            .collect();
        let rail_html = if rail {
            format!(
                r##"<nav class="rail" aria-label="{contents}"><h2>{contents}</h2><ol>{toc_list}</ol></nav>"##
            )
        } else {
            String::new()
        };
        let toc_html = if rail {
            format!(
                r##"<nav class="toc" aria-label="{contents}"><h2>{contents}</h2><ol>{toc_inline}</ol></nav>"##
            )
        } else {
            String::new()
        };
        let drawer_class = if rail { " has-rail" } else { "" };
        let foot_nav: String = PAGES
            .iter()
            .zip(ui.nav.iter())
            .map(|(p, label)| format!(r##"<a href="{p}.html">{label}</a>"##))
            .collect();
        let body = body.trim();

        format!(
            r##"<!doctype html>
<html lang="{lang}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} · AutoUpload</title>
<meta name="description" content="{desc}">
<meta name="theme-color" content="#06080c">
<link rel="alternate" hreflang="en" href="{alt_en}">
<link rel="alternate" hreflang="es" href="{alt_es}">
<link rel="icon" href="{pre}favicon.svg" type="image/svg+xml">
<link rel="preload" href="{pre}fonts/barlow-latin-400-normal.woff2" as="font" type="font/woff2" crossorigin>
<link rel="preload" href="{pre}fonts/barlow-condensed-latin-700-normal.woff2" as="font" type="font/woff2" crossorigin>
{THEME_BOOT}
<link rel="stylesheet" href="{pre}style.css?v={style_v}">
<script src="{pre}site.js?v={site_v}" defer></script>
</head>
<body>
<a class="skip" href="#main">{skip}</a>
<header class="masthead">
<div class="masthead-in">
<a class="seal" href="index.html">{SEAL}<span class="seal-text"><b>AutoUpload</b><span>{by}</span></span></a>
<div class="bar">
<nav class="tabs" aria-label="AutoUpload">{tabs}</nav>
<div class="tools">{tools}</div>
</div>
</div>
</header>
<div class="drawer{drawer_class}">
{rail_html}
<main id="main" class="card">
<div class="card-head">
<p class="callno"><span class="stamp">{callno}</span><span>{updated}</span></p>
<h1>{h1}</h1>
<p class="lede">{lede}</p>
</div>
<div class="card-body">
{toc_html}
{body}
</div>
</main>
</div>
<footer class="foot">
<div class="foot-in">
<nav aria-label="Footer">{foot_nav}<a href="{repo}" rel="noopener">GitHub</a><a href="mailto:{EMAIL}">{EMAIL}</a></nav>
<p>{foot}<br>© 2026 Ellipsio Studio · MIT License.</p>
</div>
</footer>
</body>
</html>
"##
        )
    }
}

/// Collects (id, number, label) for every `<h2 id="...">` heading in the body.
fn toc_items(body: &str) -> Vec<(String, String, String)> {
    let heading =
        Regex::new(r#"(?s)<h2 id="([^"]+)">(.*?)</h2>"#).expect("heading pattern is valid");
    let number = Regex::new(r#"<span class="no">(.*?)</span>"#).expect("number pattern is valid");
    let number_span =
        Regex::new(r#"<span class="no">.*?</span>"#).expect("number span pattern is valid");
    let tag = Regex::new(r"<[^>]+>").expect("tag pattern is valid");
    heading
        .captures_iter(body)
        .map(|c| {
            let id = c[1].to_string();
            let inner = &c[2];
            let no = number
                .captures(inner)
                .map(|n| n[1].to_string())
                .unwrap_or_default();
            let without_number = number_span.replace_all(inner, "");
            let label = tag.replace_all(&without_number, "").trim().to_string();
            (id, no, label)
        })
        .collect()
}

/// Same escaping as Python's html.escape(text, quote=True).
fn html_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(ch),
        }
    }
    out
}

fn main() {
    let site = Site::locate();
    for ui in LANGS {
        let dest = if ui.code == "en" {
            site.out.clone()
        } else {
            site.out.join("es")
        };
        fs::create_dir_all(&dest)
            .unwrap_or_else(|e| panic!("cannot create {}: {e}", dest.display()));
        for name in PAGES {
            let html = site.page(ui, name);
            let path = dest.join(format!("{name}.html"));
            fs::write(&path, html.as_bytes())
                .unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
            println!("wrote {}", site.display(&path));
        }
    }
}
