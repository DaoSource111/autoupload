# Progreso — AutoUpload
_Actualizado: 2026-11-21 · Versión publicada: v1.1.0 · Nota de la auditoría: 6,5/10_

## Estado actual
Fase 1 casi completa (pendiente de publicar): tests al día (97 pasan), carpeta de datos fuera del venv,
zonas IANA, `autoupload doctor` y CI. Falta: ruff y cobertura en la CI, y ver la CI en verde en GitHub.
Objetivo: llegar a 9–9,5 técnico por fases.

## Hecho
- v1.1.0: cola por carpetas, franjas por canal, subida privada programada, miniatura, subtítulos, playlist,
  estado SQLite por etapas, deduplicación por SHA-256, comprobación del canal del token, tarea diaria de Windows.
- Web pública en inglés y español (inicio, instalación, privacidad, términos, soporte) y marca.
- 2026-11-21: configuración Omega (CLAUDE.md, docs/progress.md, decisions.md, known-issues.md).
- 2026-11-21 · Fase 1: tests con fechas relativas; `root()` sin venv (`Documents\AutoUpload` o el repositorio) con
  aviso de datos antiguos; zonas IANA + `tzdata` en Windows; `autoupload doctor`; CI (pytest en Win/Mac/Linux ×
  Python 3.10/3.13/3.14 y test del wheel instalado); guía y README con `pip install -e .`; cuota revisada (era correcta).

## Pendiente (por sistema · [F1]–[F4] = fase)

### Calidad (tests, CI, empaquetado)
- [F1] Añadir ruff y cobertura a la CI; comprobar que la CI pasa en GitHub.
- [F2] mypy --strict, pre-commit (ruff, gitleaks), Dependabot, lockfile.
- [F2] Tests de cortes de red, cambio de horario, cuota a mitad de lote, lock y migraciones; Hypothesis para `next_slots`.
- [F2] Test de integración opcional contra un canal de pruebas.
- [F3] Releases firmadas con checksums y attestations; publicar en PyPI (`pipx install autoupload`) y winget.
- [F3] Instalador `.exe` (PyInstaller o Nuitka).

### Canales y calendario
- [F2] `autoupload migrate` para mover datos de 1.1.0 desde el venv (hoy `doctor` solo avisa).
- [F2] Horas inexistentes en el salto de primavera (ver known-issues).
- [F4] Exportar el calendario a `.ics`; reprogramar o cancelar vídeos ya programados.
- [F4] Reglas propias para Shorts.

### Publicación y estado
- [F2] Parar antes de agotar la cuota (avisar si el plan supera las unidades restantes).
- [F2] Reanudar: completar las etapas pendientes y mover el archivo, en vez de dar error.
- [F2] Marca `autoupload:<sha8>` para recuperar el `video_id` si se pierde la respuesta.
- [F2] SQLite en WAL y esquema versionado con migraciones.
- [F4] Repartir la cuota entre varios canales.

### Cliente YouTube y subida
- [F2] Guardar la URI de la sesión reanudable y continuar desde el último byte.
- [F2] Clasificar errores por `error.reason`; reintentar 429 respetando `Retry-After`.
- [F2] Detectar canales no verificados (miniaturas, vídeos de más de 15 min) con un mensaje claro.

### Cola
- [F2] Lockfile por canal; no subir archivos que aún se están copiando (tamaño estable).
- [F3] Ficha opcional (título = nombre del archivo); plantillas con variables.
- [F4] Modo vigilante de la carpeta `queue/`.

### Autenticación OAuth
- [F3] Token en el gestor de credenciales del sistema (`keyring`); revisar si los 3 ámbitos son mínimos.
- [F3] Escritura atómica del token.

### CLI y ejecución programada
- [F2] `doctor --online`: comprobar la API habilitada y renovar el token de verdad.
- [F2] Logs JSON con rotación; `--quiet`.
- [F3] `autoupload init` (asistente) e `install-schedule` multiplataforma, sustituyendo los `.ps1`.
- [F3] Notificaciones (aviso de Windows y webhook); interfaz en español e inglés con gettext.
- [F4] GUI mínima (bandeja o app local).
- [F4] Metadatos con IA opcional (clave del usuario, desactivado por defecto).

### Web pública y marca
- [F1] Publicar una release 1.2.0 con estos cambios.
- [F3] Demo en GIF, CHANGELOG, CONTRIBUTING, SECURITY.md, plantillas de issues, FAQ y solución de problemas.
- [F3] Referencia completa en MkDocs; página de comparación honesta con otras herramientas.
- [F4] Difusión (r/NewTubers, Show HN) y respuesta rápida a issues.
