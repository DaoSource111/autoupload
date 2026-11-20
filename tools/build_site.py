"""Build the static AutoUpload site (GitHub Pages) from web/_src fragments.

Each fragment starts with a header comment of `key: value` lines (title, h1, callno, desc, lede, rail),
followed by the body HTML. `<h2 id="...">` headings build the contents list.
`{PERMS}` inserts _src/<lang>/_perms.html; `{REPO}` and `{ZIP}` in a fragment are replaced with the public repository links below.
English pages go to web/ (docs/ in the public repository), Spanish pages to its es/ folder.

Run: python tools/build_site.py
"""
import hashlib
import html
import re
from pathlib import Path

# Public repository of the app (change here if the GitHub account or repository name differs).
GITHUB_OWNER = "DaoSource111"
GITHUB_REPO = "autoupload"
REPO = f"https://github.com/{GITHUB_OWNER}/{GITHUB_REPO}"
ZIP = f"{REPO}/archive/refs/heads/main.zip"
EMAIL = "ellipsio.studio@gmail.com"

ROOT = Path(__file__).resolve().parent.parent
# The site lives in web/ in the working copy and in docs/ in the public repository (served by GitHub Pages).
OUT = ROOT / "docs" if (ROOT / "docs" / "_src").exists() else ROOT / "web"
SRC = OUT / "_src"
PAGES = ["index", "install", "privacy", "terms", "support"]
UPDATED = {"en": "Last updated: November 20, 2026", "es": "Última actualización: 20 de noviembre de 2026"}
UI = {
    "en": {
        "nav": ["Home", "Install", "Privacy", "Terms", "Support"],
        "skip": "Skip to content", "contents": "Contents", "other": "ES", "other_label": "Leer en español",
        "by": "by <em>Ellipsio Studio</em>", "to_dark": "Switch to night view", "to_light": "Switch to light view",
        "source": "Source code on GitHub",
        "foot": "AutoUpload is a free tool by Ellipsio Studio, Peru. It is not affiliated with, endorsed or "
                "sponsored by Google LLC or YouTube. This site sets no cookies and loads nothing from third parties.",
    },
    "es": {
        "nav": ["Inicio", "Instalación", "Privacidad", "Términos", "Soporte"],
        "skip": "Ir al contenido", "contents": "Contenido", "other": "EN", "other_label": "Read in English",
        "by": "de <em>Ellipsio Studio</em>", "to_dark": "Cambiar a vista noche", "to_light": "Cambiar a vista clara",
        "source": "Código fuente en GitHub",
        "foot": "AutoUpload es una herramienta gratuita de Ellipsio Studio, Perú. No está afiliada, respaldada ni "
                "patrocinada por Google LLC ni YouTube. Este sitio no usa cookies ni carga nada de terceros.",
    },
}

# AutoUpload mark: a cobalt tile holding an "A" drawn as an upward stroke; its crossbar is the mint
# time slot the video lands in. Deliberately unrelated to the Source Grade ring monogram.
SEAL = ('<svg viewBox="0 0 100 100" aria-hidden="true">'
        '<rect x="3" y="3" width="94" height="94" rx="24" fill="#1f3bff"/>'
        '<rect x="3" y="3" width="94" height="94" rx="24" fill="none" stroke="#06080c" stroke-opacity=".25" stroke-width="2"/>'
        '<path d="M27 76 L50 25 L73 76" fill="none" stroke="#ffffff" stroke-width="10" stroke-linecap="round" stroke-linejoin="round"/>'
        '<path d="M34 59 H68" fill="none" stroke="#5dfbd3" stroke-width="9" stroke-linecap="round"/>'
        '<circle cx="80" cy="59" r="4.5" fill="#5dfbd3"/>'
        '</svg>')
GH = ('<svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8z"/></svg>')
SUN = ('<svg class="sun" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">'
       '<circle cx="12" cy="12" r="4.5"/><path d="M12 1.5v2.5M12 20v2.5M1.5 12H4M20 12h2.5M4.6 4.6l1.8 1.8M17.6 17.6l1.8 1.8M4.6 19.4l1.8-1.8M17.6 6.4l1.8-1.8"/></svg>')
MOON = ('<svg class="moon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" aria-hidden="true">'
        '<path d="M20.5 14.5A8.5 8.5 0 0 1 9.5 3.5a8.5 8.5 0 1 0 11 11z"/></svg>')
# Applied before first paint so a saved choice never flashes the other theme.
THEME_BOOT = ('<script>try{var t=localStorage.getItem("au-theme");if(t==="dark"||t==="light")'
              'document.documentElement.setAttribute("data-theme",t)}catch(e){}</script>')


def parse(path):
    text = path.read_text(encoding="utf-8")
    m = re.match(r"<!--(.*?)-->\s*", text, re.S)
    meta = {}
    for line in m.group(1).strip().splitlines():
        k, _, v = line.partition(":")
        meta[k.strip()] = v.strip()
    body = text[m.end():]
    if "{PERMS}" in body:  # shared permissions block, one per language
        body = body.replace("{PERMS}", (path.parent / "_perms.html").read_text(encoding="utf-8").strip())
    body = body.replace("{REPO}", REPO).replace("{ZIP}", ZIP).replace("{EMAIL}", EMAIL)
    return meta, body


def toc_items(body):
    items = []
    for hid, inner in re.findall(r'<h2 id="([^"]+)">(.*?)</h2>', body, re.S):
        no = re.search(r'<span class="no">(.*?)</span>', inner)
        label = re.sub(r"<[^>]+>", "", re.sub(r'<span class="no">.*?</span>', "", inner)).strip()
        items.append((hid, no.group(1) if no else "", label))
    return items


def ver(name):
    """Short content hash so browsers fetch a changed file instead of a cached one."""
    return hashlib.sha1((OUT / name).read_bytes()).hexdigest()[:8]


def page(lang, name):
    meta, body = parse(SRC / lang / f"{name}.html")
    ui = UI[lang]
    pre = "../" if lang == "es" else ""
    other_lang = "es" if lang == "en" else "en"
    other = ("es/" if lang == "en" else "../") + f"{name}.html"
    cur = ' aria-current="page"'
    tabs = "".join(f'<a href="{p}.html"{cur if p == name else ""}>{label}</a>' for p, label in zip(PAGES, ui["nav"]))
    tools = (f'<a href="{REPO}" rel="noopener" aria-label="{ui["source"]}" title="{ui["source"]}">{GH}</a>'
             f'<a href="{other}" hreflang="{other_lang}" lang="{other_lang}" data-lang="{other_lang}" '
             f'aria-label="{ui["other_label"]}" title="{ui["other_label"]}">{ui["other"]}</a>'
             f'<button class="theme" type="button" data-to-dark="{ui["to_dark"]}" data-to-light="{ui["to_light"]}" '
             f'aria-label="{ui["to_dark"]}">{SUN}{MOON}</button>')
    items = toc_items(body)
    rail = meta.get("rail") == "yes" and items
    toc_list = "".join(f'<li><a href="#{h}"><span>{html.escape(n)}</span>{html.escape(l)}</a></li>' for h, n, l in items)
    toc_inline = "".join(f'<li><a href="#{h}">{html.escape(l)}</a></li>' for h, n, l in items)
    rail_html = (f'<nav class="rail" aria-label="{ui["contents"]}"><h2>{ui["contents"]}</h2><ol>{toc_list}</ol></nav>'
                 if rail else "")
    toc_html = (f'<nav class="toc" aria-label="{ui["contents"]}"><h2>{ui["contents"]}</h2><ol>{toc_inline}</ol></nav>'
                if rail else "")
    foot_nav = "".join(f'<a href="{p}.html">{label}</a>' for p, label in zip(PAGES, ui["nav"]))
    alt_en = f'{"" if lang == "en" else "../"}{name}.html'
    alt_es = f'{"es/" if lang == "en" else ""}{name}.html'
    h1 = meta.get("h1") or meta["title"]
    return f"""<!doctype html>
<html lang="{lang}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{meta["title"]} · AutoUpload</title>
<meta name="description" content="{html.escape(meta["desc"])}">
<meta name="theme-color" content="#06080c">
<link rel="alternate" hreflang="en" href="{alt_en}">
<link rel="alternate" hreflang="es" href="{alt_es}">
<link rel="icon" href="{pre}favicon.svg" type="image/svg+xml">
<link rel="preload" href="{pre}fonts/barlow-latin-400-normal.woff2" as="font" type="font/woff2" crossorigin>
<link rel="preload" href="{pre}fonts/barlow-condensed-latin-700-normal.woff2" as="font" type="font/woff2" crossorigin>
{THEME_BOOT}
<link rel="stylesheet" href="{pre}style.css?v={ver("style.css")}">
<script src="{pre}site.js?v={ver("site.js")}" defer></script>
</head>
<body>
<a class="skip" href="#main">{ui["skip"]}</a>
<header class="masthead">
<div class="masthead-in">
<a class="seal" href="index.html">{SEAL}<span class="seal-text"><b>AutoUpload</b><span>{ui["by"]}</span></span></a>
<div class="bar">
<nav class="tabs" aria-label="AutoUpload">{tabs}</nav>
<div class="tools">{tools}</div>
</div>
</div>
</header>
<div class="drawer{" has-rail" if rail else ""}">
{rail_html}
<main id="main" class="card">
<div class="card-head">
<p class="callno"><span class="stamp">{meta["callno"]}</span><span>{UPDATED[lang]}</span></p>
<h1>{h1}</h1>
<p class="lede">{meta["lede"]}</p>
</div>
<div class="card-body">
{toc_html}
{body.strip()}
</div>
</main>
</div>
<footer class="foot">
<div class="foot-in">
<nav aria-label="Footer">{foot_nav}<a href="{REPO}" rel="noopener">GitHub</a><a href="mailto:{EMAIL}">{EMAIL}</a></nav>
<p>{ui["foot"]}<br>© 2026 Ellipsio Studio · MIT License.</p>
</div>
</footer>
</body>
</html>
"""


def main():
    for lang in ("en", "es"):
        dest = OUT if lang == "en" else OUT / "es"
        dest.mkdir(exist_ok=True)
        for name in PAGES:
            (dest / f"{name}.html").write_text(page(lang, name), encoding="utf-8", newline="\n")
            print("wrote", (dest / f"{name}.html").relative_to(ROOT))


if __name__ == "__main__":
    main()
