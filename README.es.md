<p align="center"><img src="assets/brand/autoupload-logo-rounded-512.png" width="112" alt="AutoUpload logo"></p>

<h1 align="center">AutoUpload</h1>

<p align="center">Sube y programa videos de YouTube desde tu propia computadora. Sin servidor, sin cuentas, con tu propia cuota.</p>

<p align="center">
<a href="https://github.com/DaoSource111/autoupload/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/DaoSource111/autoupload"></a>
<img alt="Windows, macOS, Linux" src="https://img.shields.io/badge/Windows%20%C2%B7%20macOS%20%C2%B7%20Linux-native-1f3bff">
<img alt="Written in Rust" src="https://img.shields.io/badge/rust-1.80%2B-orange">
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

## Descarga

Descarga el archivo de tu sistema desde la [última versión](https://github.com/DaoSource111/autoupload/releases/latest).
Cada uno incluye la aplicación de escritorio (`autoupload-gui`) y la línea de comandos (`autoupload`), ambas nativas, sin nada más que instalar:

| Sistema | Archivo | Primer inicio |
|---|---|---|
| Windows 10/11 (x64) | `AutoUpload-<version>-windows-x64.zip` | descomprime y abre `autoupload-gui.exe`; si SmartScreen avisa, elige *Más información → Ejecutar de todas formas* |
| macOS 11+ (Apple silicon) | `AutoUpload-<version>-macos-arm64.zip` | mueve `AutoUpload.app` a Aplicaciones y, la primera vez, haz clic derecho → *Abrir* |
| macOS 11+ (Intel) | `AutoUpload-<version>-macos-x64.zip` | igual que arriba |
| Linux (x64, glibc 2.35+) | `AutoUpload-<version>-linux-x64.tar.gz` | descomprime y ejecuta `./install.sh` (lo añade al menú de aplicaciones) o `./autoupload-gui` |

Las compilaciones aún no están firmadas digitalmente, de ahí los avisos. Verifica una descarga con `SHA256SUMS` o con
`gh attestation verify <archivo> --repo DaoSource111/autoupload`.

## Antes de la primera subida

Cada usuario crea su propio cliente OAuth de tipo **App de escritorio** en Google Cloud, habilita la YouTube Data API v3
y guarda el archivo del cliente como `config/client_secret.json` dentro de la carpeta de datos. La guía paso a paso está en
https://daosource111.github.io/autoupload/es/install.html. La aplicación de escritorio muestra la carpeta de datos en su barra lateral y
su panel **Diagnostics** indica exactamente qué falta; `autoupload doctor` hace lo mismo en una terminal.

## Aplicación de escritorio

Abre **AutoUpload**, crea un canal, pulsa **Connect** para iniciar sesión con Google, coloca los videos con su archivo YAML en la
carpeta de cola del canal y pulsa **Preview dates** para ver el plan. **Upload** pide confirmación antes de que nada salga de tu
computadora. La pestaña **Calendar** muestra lo que ya está programado en YouTube.

## Línea de comandos

```powershell
autoupload doctor                         # revisa la instalación y muestra la carpeta de datos
autoupload channel new mychannel
autoupload auth mychannel login
autoupload schedule mychannel             # vista previa: no sube nada
autoupload schedule mychannel --yes       # sube
autoupload calendar mychannel             # lo que está programado
autoupload data-folder D:\Videos\AutoUpload   # guarda canales y configuración en otro lugar
```

Códigos de salida: `0` correcto, `2` entrada o configuración no válida, `3` autenticación, `4` error de YouTube.

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
`upload-all.ps1` (`autoupload schedule --all --yes`) cada día y guarda un registro en `logs\`. En macOS y Linux,
añade el mismo comando a `crontab -e`, p. ej. `0 10 * * * ~/.local/bin/autoupload schedule --all --yes >> ~/autoupload.log 2>&1`.

## Seguridad

- `config/client_secret.json`, los tokens OAuth y los datos de los canales nunca deben subirse a un repositorio. `.gitignore` los excluye.
- Los tokens OAuth se guardan en `%APPDATA%\AutoUpload` en Windows y en `~/.config/autoupload` en macOS y Linux
  (se cambia con `AUTOUPLOAD_CONFIG_DIR`); en macOS y Linux, solo tu usuario puede leer esos archivos.
- `channels/` y `config/` están en la carpeta de datos, elegida en este orden: `AUTOUPLOAD_HOME`; la carpeta que fijes con
  `autoupload data-folder` o con el botón **Change…** de la app; una carpeta `channels/` o `config/` junto al programa
  (copia portátil); si no, `Documentos\AutoUpload` (`~/AutoUpload` en macOS y Linux).
- La versión 2 lee sin cambios los tokens, los archivos de canales y el historial de subidas de la versión 1.x.

## Compilar desde el código

Requiere Rust 1.80 o posterior (y en Linux `libfontconfig1-dev libxkbcommon-dev`):

```powershell
cargo test --workspace
cargo build --release          # target/release/autoupload y autoupload-gui
```

## Estructura del repositorio

| Ruta | Contenido |
|---|---|
| `crates/core/` | el motor: canales, cola, OAuth, cliente de YouTube, subida reanudable, estado (`cargo test`) |
| `crates/cli/` | la línea de comandos `autoupload` |
| `crates/gui/` | la aplicación de escritorio (Slint) |
| `packaging/` | entrada de menú e instalador de Linux, metadatos del paquete de macOS |
| `src/autoupload/`, `tests/` | la versión 1.x en Python, que se mantiene hasta que la 2.0 esté probada con subidas reales |
| `docs/` | el sitio web público, servido por GitHub Pages; edita `docs/_src/` y ejecuta `python tools/build_site.py` |
| `install-task.ps1`, `upload-all.ps1` | pasada diaria opcional como tarea programada de Windows |

## Licencia

MIT © 2026 Ellipsio Studio. AutoUpload no está afiliado, respaldado ni patrocinado por Google LLC ni YouTube.
