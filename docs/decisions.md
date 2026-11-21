# Decisiones de arquitectura
Plantilla: `## AAAA-MM-DD — Título` · **Decisión** · **Motivo** · **Alternativas descartadas**

## 2026-11-20 — Solo la API oficial de YouTube
**Decisión:** usar YouTube Data API v3 vía OAuth; nada de automatizar el navegador.
**Motivo:** legal según los términos de servicio, estable y verificable por Google.
**Descartado:** Playwright o Selenium contra Studio (se rompe y va contra los términos).

## 2026-11-20 — Todo local, con el cliente OAuth del usuario
**Decisión:** sin servidor; cada usuario crea su cliente OAuth "Desktop app" y usa su propia cuota.
**Motivo:** privacidad, coste cero y sin compartir cuota.
**Coste aceptado:** configurar Google Cloud es difícil para usuarios no técnicos.

## 2026-11-20 — Seguro por defecto
**Decisión:** plan sin cambios salvo con `--yes`; nada público sin `--allow-public`; comprobar el canal del token.
**Motivo:** un error sube contenido real a un canal real.

## 2026-11-20 — Estado SQLite indexado por (sha256, channel_id) con etapas
**Decisión:** registrar cada etapa (validated → uploaded → thumbnail → captions → playlist → verified).
**Motivo:** reanudar sin volver a subir y no duplicar el mismo archivo.

## 2026-11-20 — Fichas YAML estrictas
**Decisión:** las claves desconocidas en `channel.yaml` y en las fichas son error.
**Motivo:** que una errata se detecte en vez de ignorarse sin aviso.

## 2026-11-21 — Carpeta de datos fuera del paquete instalado
**Decisión:** `root()` = `AUTOUPLOAD_HOME` → repositorio si se ejecuta desde el código (`pyproject.toml` + `src/autoupload`)
→ `Documents\AutoUpload` (`~/AutoUpload` fuera de Windows). La guía instala con `pip install -e .`, así que en el flujo
habitual (clonar y ejecutar) el repositorio sigue siendo la carpeta de datos.
**Motivo:** con `pip install .` los datos acababan en `.venv\Lib\` y se perdían al recrear el venv.
**Descartado:** `%APPDATA%` (carpeta oculta y móvil: mala para dejar vídeos grandes); el directorio actual (depende de dónde se lance).

## 2026-11-21 — Zonas IANA, sin dejar de aceptar offsets
**Decisión:** `timezone` acepta `Europe/Madrid` (con `zoneinfo`) o `+01:00`. `tzdata` es dependencia solo en Windows.
**Motivo:** con offsets fijos las franjas se movían una hora con cada cambio de horario; los `channel.yaml` existentes siguen valiendo.

## 2026-11-21 — Notas de desarrollo en docs/, fuera de la web
**Decisión:** `progress.md`, `decisions.md` y `known-issues.md` viven en `docs/` y se excluyen de GitHub Pages en `docs/_config.yml`.
**Motivo:** mantener la convención Omega sin publicar las notas como páginas de la web.
