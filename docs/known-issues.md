# Bugs conocidos sin resolver
Plantilla: `## Título` · **Síntoma** · **Causa** · **Archivo** · **Detectado** · **Gravedad**

## Hora inexistente en el cambio de horario — BAJA
**Síntoma:** con zona IANA, una franja dentro del salto de primavera (p. ej. 02:30 en Europe/Madrid) se calcula con el
desfase anterior al cambio. **Archivo:** src/autoupload/channels.py (`_slots`) · **Detectado:** 2026-11-21

## Reanudación real no probada contra YouTube — MEDIA (por verificar)
**Síntoma posible:** la reanudación de subidas usa el camino interno de googleapiclient (`resumable_uri` +
`_in_error_state`); está cubierta con dobles de prueba, pero no se ha probado cortando una subida real.
**Archivo:** src/autoupload/upload.py · **Detectado:** 2026-11-21

---
## Resueltos
- 2026-11-21 · 4 tests caducados por fecha fija → fechas relativas al reloj real.
- 2026-11-21 · `root()` apuntaba dentro del venv → AUTOUPLOAD_HOME, repositorio o `Documents\AutoUpload`; `doctor` avisa y `migrate` mueve los datos.
- 2026-11-21 · Sin horario de verano → zonas IANA con `zoneinfo` (+ `tzdata` en Windows).
- 2026-11-21 · "Cuota mal estimada": falso positivo; Google confirma 100 `videos.insert` al día en un bucket propio.
- 2026-11-21 · Subida interrumpida empezaba de cero → la sesión se guarda y se reanuda; si se pierde la respuesta, se busca el vídeo por nombre y tamaño (`fileDetails`).
- 2026-11-21 · Reanudar tras fallo a medias daba error de duplicado → se completan las etapas y se mueve el archivo.
- 2026-11-21 · Errores de cuota por texto → clasificación por `error.reason`; `rateLimitExceeded` ya no para la ejecución.
- 2026-11-21 · Miniatura rechazada (canal sin verificar) hacía fallar el vídeo → aviso con solución, el vídeo sigue.
- 2026-11-21 · PRODUCT.md desactualizado → corregido (archivo local, fuera de git).
