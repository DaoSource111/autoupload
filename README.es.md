<p align="center"><img src="assets/brand/autoupload-logo-rounded-512.png" width="112" alt="AutoUpload logo"></p>

<h1 align="center">AutoUpload</h1>

<p align="center">Sube y programa videos de YouTube desde tu propia computadora. Sin servidor, sin cuentas, con tu propia cuota.</p>

<p align="center">
<a href="https://github.com/DaoSource111/autoupload/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/DaoSource111/autoupload"></a>
<img alt="Python 3.10+" src="https://img.shields.io/badge/python-3.10%2B-1f3bff">
<img alt="YouTube Data API v3" src="https://img.shields.io/badge/YouTube%20Data%20API-v3-5dfbd3">
<a href="LICENSE"><img alt="MIT license" src="https://img.shields.io/badge/license-MIT-lightgrey"></a>
</p>

[English](README.md) · **Español**

AutoUpload es una herramienta de escritorio gratuita de **Ellipsio Studio** que sube y programa videos en los
canales de YouTube que conectes, con la API oficial YouTube Data v3. Funciona por completo en tu computadora:
no hay servidor de AutoUpload ni base de datos externa. Tus datos solo van a Google.

El programa, sus comandos, carpetas y opciones están en inglés; esta página y el sitio web tienen versión en español.

- Sitio web, guía de instalación, política de privacidad y condiciones: https://daosource111.github.io/autoupload/es/
- Soporte: ellipsio.studio@gmail.com

## Qué hace

Pones cada video en la carpeta de cola de tu canal (`channels/<canal>/queue/`) con un pequeño archivo YAML
con su título y sus datos. AutoUpload:

1. lo sube como video **privado**;
2. le asigna el **siguiente horario libre** de ese canal (o la fecha fija que indiques), para que YouTube lo haga público a esa hora;
3. le pone miniatura, subtítulos y lista de reproducción;
4. mueve el archivo a `uploaded/` con un informe, y nunca sube dos veces el mismo archivo.

Nunca sube sin `--yes`, nunca publica al instante sin `--allow-public` y se niega a subir si el token
conectado pertenece a otro canal.

## Permisos

AutoUpload pide exactamente tres permisos OAuth de Google: `youtube.upload`, `youtube.force-ssl` y
`youtube.readonly`. Para qué sirve cada uno se explica en el sitio web.

## Inicio rápido (Windows, Python 3.10+)

```powershell
py -m venv .venv
.\.venv\Scripts\pip install -e .
.\.venv\Scripts\autoupload.exe doctor                    # revisa la instalación y muestra la carpeta de datos
.\.venv\Scripts\autoupload.exe channel new mychannel
.\.venv\Scripts\autoupload.exe auth mychannel login
.\.venv\Scripts\autoupload.exe schedule mychannel           # vista previa: no sube nada
.\.venv\Scripts\autoupload.exe schedule mychannel --yes     # sube
```

Antes de `auth … login`, crea tu propio cliente OAuth de tipo **App de escritorio** en Google Cloud, habilita
la YouTube Data API v3 y guarda el archivo del cliente como `config\client_secret.json` dentro de la carpeta de datos
(`Documentos\AutoUpload` en una copia instalada; la carpeta del repositorio si lo ejecutas desde el código con
`pip install -e .`; `autoupload doctor` la muestra). La guía paso a paso,
para hacerlo a mano o con un agente de IA, está en https://daosource111.github.io/autoupload/es/install.html

## Configuración del canal (`channels/<canal>/channel.yaml`)

| Clave | Por defecto | Significado |
|---|---|---|
| `profile` | nombre del canal | nombre del token OAuth |
| `client_secret` | `../../config/client_secret.json` | archivo del cliente OAuth de escritorio |
| `timezone` | `"+00:00"` | zona de `hours`: un nombre que sigue el horario de verano (`Europe/Madrid`, `America/Lima`) o un desfase fijo respecto a UTC (`+00:00`, `-05:00`) |
| `hours` | `["08:00"]` | horas de publicación de cada día |
| `days_between` | `1` | 1 = cada día, 7 = una vez por semana |
| `min_lead_hours` | `3` | nunca programar más cerca de ahora que esto |
| `max_per_run` | `5` | subidas por pasada |
| `language` | `en` | idioma del título, la descripción y el audio |
| `category` | `"27"` | ID de categoría de YouTube (27 Educación, 28 Ciencia y tecnología, 24 Entretenimiento) |
| `made_for_kids` | `false` | declaración «hecho para niños» |
| `contains_ai` | `null` | declaración de contenido alterado o sintético |
| `notify_subscribers` | `false` | avisar a los suscriptores al publicar |
| `playlist` | `null` | ID de lista para cada subida |
| `base_tags` | `[]` | etiquetas que se añaden a todos los videos |
| `description_footer` | `""` | texto que se añade al final de cada descripción |

Las claves desconocidas se rechazan: una errata se avisa en lugar de ignorarse en silencio.

## Ficha del video (`<video>.yaml` junto a cada video)

Solo `title` es obligatoria. Claves habituales: `description`, `tags`, `thumbnail`, `publish_at` (ISO 8601 con
desfase, p. ej. `2026-11-21T08:00:00+00:00`), `playlist_id`, `privacy`, `category_id`, `default_language`,
`made_for_kids`, `contains_synthetic_media`, `notify_subscribers`, `captions`, `localizations`.

## Pasada diaria (opcional)

`install-task.ps1 -Time 10:00` registra una tarea programada de Windows llamada `AutoUpload` que ejecuta
`upload-all.ps1` (`autoupload schedule --all --yes`) cada día y guarda un registro en `logs\`.

## Seguridad

- `config/client_secret.json`, los tokens OAuth y los datos de los canales nunca deben subirse a un repositorio. `.gitignore` los excluye.
- Los tokens OAuth se guardan en `%APPDATA%\AutoUpload` en tu propia computadora (se cambia con `AUTOUPLOAD_CONFIG_DIR`).
  `channels/` y `config/` están en la carpeta de datos: `AUTOUPLOAD_HOME` si está definida; si no, el repositorio
  cuando se ejecuta desde el código; si no, `Documentos\AutoUpload` (`~/AutoUpload` en macOS/Linux). La versión 1.1.0
  los guardaba dentro del venv (`.venv\Lib\`) al instalar con `pip install .`; `autoupload doctor` lo detecta y dice adónde moverlos.

## Estructura del repositorio

| Ruta | Contenido |
|---|---|
| `src/autoupload/` | la aplicación |
| `tests/` | pruebas (`pip install .[test]` y luego `pytest`) |
| `docs/` | el sitio web público, servido por GitHub Pages; edita `docs/_src/` y ejecuta `python tools/build_site.py` |
| `install-task.ps1`, `upload-all.ps1` | pasada diaria opcional como tarea programada de Windows |

## Licencia

MIT © 2026 Ellipsio Studio. AutoUpload no está afiliado, respaldado ni patrocinado por Google LLC ni YouTube.
