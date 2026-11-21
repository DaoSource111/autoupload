# Progreso — AutoUpload
_Actualizado: 2026-11-21 · Versión publicada: v1.1.0 · Nota de la auditoría: 6,5/10_

## Estado actual
v1.2.0 lista en local (sin publicar): fases 1 y 2 hechas. 130 tests, CI preparada (no ejecutada aún en GitHub).
Nota técnica estimada: 8,5/10. Lo que falta para 10 es sobre todo uso real (madurez) y facilidad para no técnicos.

## Hecho
- v1.1.0: cola por carpetas, franjas por canal, subida privada programada, miniatura, subtítulos, playlist,
  estado SQLite por etapas, deduplicación por SHA-256, comprobación del canal del token, tarea diaria de Windows.
- Web pública en inglés y español y marca.
- 2026-11-21 · Configuración Omega (CLAUDE.md y docs/).
- 2026-11-21 · Fase 1: tests sin fecha fija; carpeta de datos fuera del venv; zonas IANA; `doctor`; CI; guía con `pip install -e .`.
- 2026-11-21 · Fase 2 (v1.2.0): subida reanudable entre ejecuciones; recuperación de respuesta perdida por `fileDetails`;
  reintentos con `Retry-After` en todas las llamadas; errores por `reason`; miniatura 403 no fatal; reanudar completa
  etapas; lock por canal; espera de archivos en copia; escritura atómica; SQLite WAL + esquema versionado; `migrate`;
  rotación de logs; CI con lint y cobertura; Dependabot; CHANGELOG, CONTRIBUTING, SECURITY, CODE_OF_CONDUCT, plantillas.

## Pendiente (por sistema · [F1]–[F4] = fase)

### Calidad (tests, CI, empaquetado)
- [F1] Comprobar que la CI pasa en GitHub (push pendiente); fijar un umbral de cobertura con el dato real.
- [F2] mypy --strict, pre-commit (ruff completo, gitleaks), lockfile.
- [F2] Hypothesis para `next_slots`.
- [F2] Test de integración opcional contra un canal de pruebas (incluye cortar una subida real y reanudarla).
- [F3] Releases firmadas con checksums y attestations; publicar en PyPI (`pipx install autoupload`) y winget.
- [F3] Instalador `.exe` (PyInstaller o Nuitka).

### Canales y calendario
- [F2] Horas inexistentes en el salto de primavera (ver known-issues).
- [F4] Exportar el calendario a `.ics`; reprogramar o cancelar vídeos ya programados.
- [F4] Reglas propias para Shorts.

### Publicación y estado
- [F2] Parar antes de agotar la cuota (avisar si el plan supera las unidades restantes).
- [F4] Repartir la cuota entre varios canales.

### Cliente YouTube y subida
- [F2] Canales no verificados: aviso previo para vídeos de más de 15 min (la miniatura ya está resuelta).

### Cola
- [F3] Ficha opcional (título = nombre del archivo); plantillas con variables.
- [F4] Modo vigilante de la carpeta `queue/`.

### Autenticación OAuth
- [F3] Token en el gestor de credenciales del sistema (`keyring`); revisar si los 3 ámbitos son mínimos.
- [F3] Escritura atómica del token.

### CLI y ejecución programada
- [F2] `doctor --online`: comprobar la API habilitada y renovar el token de verdad.
- [F2] Logs JSON; `--quiet` (la rotación ya está hecha).
- [F3] `autoupload init` (asistente) e `install-schedule` multiplataforma, sustituyendo los `.ps1`.
- [F3] Notificaciones (aviso de Windows y webhook); interfaz en español e inglés con gettext.
- [F4] GUI mínima (bandeja o app local).
- [F4] Metadatos con IA opcional (clave del usuario, desactivado por defecto).

### Web pública y marca
- [F1] Publicar la release 1.2.0 (CHANGELOG listo) tras ver la CI en verde.
- [F3] Demo en GIF, CHANGELOG, CONTRIBUTING, SECURITY.md, plantillas de issues, FAQ y solución de problemas.
- [F3] Referencia completa en MkDocs; página de comparación honesta con otras herramientas.
- [F4] Difusión (r/NewTubers, Show HN) y respuesta rápida a issues.
