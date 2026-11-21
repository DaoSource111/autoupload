# Bugs conocidos sin resolver
Plantilla: `## Título` · **Síntoma** · **Causa** · **Archivo** · **Detectado** · **Gravedad**

## Una subida interrumpida empieza de cero y puede duplicarse — MEDIA
**Causa:** no se guarda la URI de la sesión reanudable; si se pierde la respuesta no hay `video_id`.
**Archivo:** src/autoupload/upload.py, publisher.py · **Detectado:** 2026-11-21

## Reanudar tras un fallo a medias da error — BAJA
**Síntoma:** "ya subido, quítalo de la cola" en vez de completar las etapas y mover el archivo.
**Archivo:** src/autoupload/queue.py · **Detectado:** 2026-11-21

## Errores de cuota detectados buscando texto — BAJA
**Causa:** `_is_stop_error` busca texto en `str(exc)` en vez de leer `error.reason`.
**Archivo:** src/autoupload/queue.py · **Detectado:** 2026-11-21

## Hora inexistente en el cambio de horario — BAJA
**Síntoma:** con zona IANA, una franja dentro del salto de primavera (p. ej. 02:30 en Europe/Madrid) se calcula con el
desfase anterior al cambio. **Archivo:** src/autoupload/channels.py (`_slots`) · **Detectado:** 2026-11-21

---
## Resueltos
- 2026-11-21 · 4 tests caducados por fecha fija → las fechas de los tests son relativas al reloj real.
- 2026-11-21 · `root()` apuntaba dentro del venv con `pip install .` → AUTOUPLOAD_HOME, repositorio o `Documents\AutoUpload`; `doctor` avisa si quedan datos en el venv.
- 2026-11-21 · Sin horario de verano → `timezone` acepta nombres IANA (`Europe/Madrid`) con `zoneinfo` y `tzdata` en Windows.
- 2026-11-21 · "Cuota mal estimada": era un falso positivo de la auditoría. Google confirma que `videos.insert` tiene su propio límite de 100 llamadas al día y coste 1; la tabla `QUOTA` es correcta.
- 2026-11-21 · PRODUCT.md desactualizado → rutas corregidas (el archivo es local y no se sube a git).
