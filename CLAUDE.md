# AutoUpload

Programador de subidas a YouTube para Windows: cada vídeo de `channels/<canal>/queue/`, con su ficha YAML,
se sube como privado y se programa en la siguiente franja libre del canal (YouTube Data API v3).
Todo local: sin servidor, con el cliente OAuth y la cuota del propio usuario. Autor: Ellipsio Studio.

**Stack:** Python ≥3.10 · google-api-python-client, google-auth-oauthlib · pydantic v2 · PyYAML · SQLite · pytest.
Web pública: HTML/CSS estático en `docs/` (GitHub Pages), generado con `tools/build_site.py`.

## Ejecutar y probar
```powershell
py -m venv .venv ; .\.venv\Scripts\pip install -e .[test]
.\.venv\Scripts\autoupload.exe channels                  # lista canales y colas
.\.venv\Scripts\autoupload.exe schedule <canal>          # plan, no sube nada
.\.venv\Scripts\autoupload.exe schedule <canal> --yes    # sube de verdad (¡canal real!)
py -3 -m pytest -q                                       # tests, sin red (FakeClient)
py -3 tools/build_site.py                                # regenera docs/ desde docs/_src/
```

Nunca ejecutes `schedule --yes`, `publish --yes` ni `auth … login` sin que el usuario lo pida: actúan sobre canales reales.

## Mapa de carpetas
| Carpeta | Contenido |
|---|---|
| `src/autoupload/` | la aplicación (paquete instalable, CLI `autoupload`) |
| `tests/` | pytest; `conftest.py` contiene `FakeClient`, un YouTube falso |
| `channels/` | datos locales por canal: `channel.yaml`, `queue/`, `uploaded/`, `state.sqlite`, `channel_id.txt` (fuera de git) |
| `config/` | `client_secret.json` del usuario (fuera de git, nunca leer ni mostrar) |
| `docs/` | web pública en GitHub Pages; **editar `docs/_src/{en,es}/`**, no el HTML generado. `progress.md`, `decisions.md` y `known-issues.md` están excluidos de la web en `docs/_config.yml` |
| `tools/` | `build_site.py` (generador de la web) |
| `assets/brand/` | logos y social preview |
| raíz | `install-task.ps1`, `upload-all.ps1` (tarea diaria de Windows), `pyproject.toml` |

## Sistemas
| Sistema | Archivos principales |
|---|---|
| Autenticación OAuth | `auth.py` |
| Cliente YouTube y subida reanudable | `youtube_client.py`, `upload.py` |
| Publicación por etapas y estado | `publisher.py`, `state.py` (esquema versionado, `PRAGMA user_version`), `metadata.py`, `fsutil.py` |
| Canales, carpeta de datos y calendario | `channels.py` (`root()`, `migrate_legacy`, zonas IANA) |
| Cola de vídeos | `queue.py` (lock por canal, espera de archivos en copia) |
| Manifiesto y validación | `manifest.py`, `validators.py`, `studio_package.py` |
| CLI, diagnóstico y ejecución programada | `cli.py`, `doctor.py`, `install-task.ps1`, `upload-all.ps1` |
| Web pública y marca | `docs/_src/`, `tools/build_site.py`, `assets/brand/` |
| Calidad y comunidad | `tests/`, `pyproject.toml`, `.github/` (CI, Dependabot, plantillas), `CHANGELOG.md`, `CONTRIBUTING.md`, `SECURITY.md` |

(Los archivos `.py` están en `src/autoupload/`.)

## Convenciones
- Código, mensajes y claves en inglés; README y web en inglés y español.
- `from __future__ import annotations` y tipado en todas las firmas; pydantic con `extra="forbid"` (las claves desconocidas son error).
- Las librerías de Google se importan dentro de las funciones, para que los tests no las necesiten.
- Toda llamada remota pasa por `YouTubeClient`; los tests usan `FakeClient`, nunca la red.
- Seguridad por defecto: nada se sube sin `--yes` y nada se publica sin `--allow-public`; el token debe ser del canal esperado.
- Códigos de salida de la CLI: `OK=0, INVALID=2, AUTH=3, REMOTE=4`.
- Commits pequeños y descriptivos en inglés, con `Co-Authored-By` cuando intervenga Claude.
