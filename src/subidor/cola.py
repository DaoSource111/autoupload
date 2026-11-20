"""Channel queue: each video in the queue folder with its sheet (.yaml) is scheduled in the next free slot and moved to the uploaded folder."""
from __future__ import annotations

import json
import re
import shutil
import unicodedata
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Callable, Optional

import yaml

from .canales import Canal, ocupadas_desde_youtube, siguientes
from .manifest import Manifest
from .validators import VIDEO_EXT

# The sheet accepts Spanish or English keys (the manifest keys).
ALIAS = {"titulo": "title", "título": "title", "descripcion": "description", "descripción": "description",
         "etiquetas": "tags", "miniatura": "thumbnail", "lista": "playlist_id", "fecha": "publish_at",
         "privacidad": "privacy", "para_ninos": "made_for_kids", "contiene_ia": "contains_synthetic_media",
         "notificar": "notify_subscribers", "categoria": "category_id", "idioma": "default_language",
         "subtitulos": "captions", "traducciones": "localizations"}
MINIATURAS = (".jpg", ".jpeg", ".png")


class Parada(RuntimeError):
    """Error that stops the whole run (authentication, quota, wrong channel)."""


@dataclass
class Elemento:
    video: Path
    ficha: Optional[Path]
    miniatura: Optional[Path]

    @property
    def id(self) -> str:
        s = unicodedata.normalize("NFKD", self.video.stem).encode("ascii", "ignore").decode().lower()
        s = re.sub(r"[^a-z0-9-]+", "-", s).strip("-")[:60]
        return s if len(s) >= 2 else f"video-{s or 'x'}"


@dataclass
class Resultado:
    hechos: list[dict] = field(default_factory=list)
    errores: list[str] = field(default_factory=list)
    plan: list[str] = field(default_factory=list)


def elementos(canal: Canal) -> list[Elemento]:
    if not canal.cola.exists():
        return []
    res = []
    for v in sorted(p for p in canal.cola.iterdir() if p.suffix.lower() in VIDEO_EXT):
        ficha = next((v.with_suffix(e) for e in (".yaml", ".yml", ".json") if v.with_suffix(e).exists()), None)
        mini = next((v.with_suffix(e) for e in MINIATURAS if v.with_suffix(e).exists()), None)
        res.append(Elemento(v, ficha, mini))
    return res


def leer_ficha(path: Path) -> dict:
    texto = path.read_text(encoding="utf-8")
    datos = json.loads(texto) if path.suffix == ".json" else yaml.safe_load(texto)
    if not isinstance(datos, dict):
        raise ValueError(f"{path.name}: the sheet must be an object with at least «title» (or «titulo»)")
    return {ALIAS.get(k, k): v for k, v in datos.items()}


def manifiesto(canal: Canal, el: Elemento, cuando: Optional[datetime]) -> Manifest:
    """Merges the channel's default values with the video's sheet. Without a fixed date, the video is private until slot `cuando`."""
    if el.ficha is None:
        raise ValueError(f"missing sheet {el.video.stem}.yaml with the title")
    f = leer_ficha(el.ficha)
    if "title" not in f:
        raise ValueError(f"{el.ficha.name}: missing «title» (or «titulo»)")
    desc = str(f.get("description", "")).rstrip()
    if canal.pie_descripcion:
        desc = f"{desc}\n\n{canal.pie_descripcion}".strip()
    tags = list(dict.fromkeys([*canal.etiquetas_base, *(f.get("tags") or [])]))
    datos = {
        "id": el.id, "video": el.video.name, "title": f["title"], "description": desc, "tags": tags,
        "category_id": str(f.get("category_id", canal.categoria)),
        "default_language": f.get("default_language", canal.idioma),
        "default_audio_language": f.get("default_audio_language", f.get("default_language", canal.idioma)),
        "made_for_kids": f.get("made_for_kids", canal.para_ninos),
        "contains_synthetic_media": f.get("contains_synthetic_media", canal.contiene_ia),
        "notify_subscribers": f.get("notify_subscribers", canal.notificar),
        "playlist_id": f.get("playlist_id", canal.lista),
        "privacy": f.get("privacy", "private"),
    }
    if f.get("thumbnail") or el.miniatura:
        datos["thumbnail"] = f.get("thumbnail") or el.miniatura.name
    for k in ("localizations", "captions"):
        if f.get(k):
            datos[k] = f[k]
    fija = f.get("publish_at")
    if fija:
        datos["publish_at"] = fija if isinstance(fija, datetime) else datetime.fromisoformat(str(fija))
    elif datos["privacy"] == "private" and cuando is not None:
        datos["publish_at"] = cuando
    return Manifest.model_validate(datos).resolve(canal.cola)


def _es_parada(exc: Exception) -> bool:
    t = str(exc)
    return any(k in t for k in ("quotaExceeded", "uploadLimitExceeded", "dailyLimitExceeded", "rateLimitExceeded"))


def procesar(canal: Canal, cliente, *, ejecutar: bool, maximo: Optional[int] = None, permitir_publico: bool = False,
             ahora: Optional[datetime] = None, aviso: Callable[[str], None] = print,
             progreso: Callable[[float], None] = lambda p: None) -> Resultado:
    """Schedules the queue. With ejecutar=False it only shows the plan (reads the channel calendar, uploads nothing)."""
    from .publisher import SafetyError, publish, write_report
    from .state import State

    res = Resultado()
    pendientes = elementos(canal)[: maximo or canal.max_por_ejecucion]
    if not pendientes:
        aviso(f"[{canal.nombre}] queue empty")
        return res

    canal_yt = cliente.my_channel()
    esperado = canal.canal_id_esperado()
    if esperado and canal_yt["id"] != esperado:
        raise Parada(f"[{canal.nombre}] the token belongs to channel «{canal_yt['title']}» ({canal_yt['id']}), "
                     f"not to {esperado}. Run: subidor auth {canal.nombre} login")
    aviso(f"[{canal.nombre}] connected channel: {canal_yt['title']} ({canal_yt['id']})")

    ocupadas = ocupadas_desde_youtube(cliente.uploads(), canal.tz)
    ranuras = siguientes(canal, len(pendientes), ocupadas, ahora)
    estado = State(canal.estado)
    for el, cuando in zip(pendientes, ranuras):
        try:
            m = manifiesto(canal, el, cuando)
        except Exception as e:  # noqa: BLE001
            res.errores.append(f"{el.video.name}: {e}")
            aviso(f"  ✗ {el.video.name}: {e}")
            continue
        fecha = m.publish_at.astimezone(canal.tz).strftime("%Y-%m-%d %H:%M") if m.publish_at else m.privacy.value
        linea = f"{el.video.name} → «{m.title}» · {fecha}"
        res.plan.append(linea)
        if not ejecutar:
            aviso(f"  · {linea}")
            continue
        aviso(f"  ↑ {linea}")
        try:
            r = publish(m, cliente, estado, confirm=True, allow_public=permitir_publico, on_progress=progreso)
        except SafetyError as e:
            res.errores.append(f"{el.video.name}: {e}")
            aviso(f"\n  ✗ blocked: {e}")
            continue
        except Exception as e:  # noqa: BLE001
            if _es_parada(e):
                raise Parada(f"[{canal.nombre}] YouTube does not accept more uploads today: {e}") from e
            res.errores.append(f"{el.video.name}: {e}")
            aviso(f"\n  ✗ error (the saved state allows retrying without duplicates): {e}")
            continue
        if r["resumed"] and any(s.startswith("videos.insert (skipped") for s in r["steps"]):
            msg = (f"{el.video.name}: this same file was already uploaded to this channel ({r['url']}); "
                   f"it will not be uploaded again. Remove it from the queue.")
            res.errores.append(msg)
            aviso(f"\n  ✗ {msg}")
            continue
        destino = canal.subidos / f"{datetime.now(timezone.utc).astimezone(canal.tz):%Y-%m-%d}_{el.id}"
        destino.mkdir(parents=True, exist_ok=True)
        write_report(r, destino / "informe.json")
        for p in (el.video, el.ficha, el.miniatura):
            if p and p.exists():
                shutil.move(str(p), str(destino / p.name))
        res.hechos.append({"video": el.video.name, "url": r["url"], "fecha": fecha, "verificado": r["verification"]["ok"]})
        aviso(f"\n  ✓ {r['url']} ({'verified' if r['verification']['ok'] else 'check: ' + ', '.join(r['verification']['problems'])})")
    return res


def calendario(canal: Canal, cliente, dias: int = 30, ahora: Optional[datetime] = None) -> list[tuple[datetime, str, str]]:
    """Channel uploads between today and `dias` days from now, plus those of the last 3 days."""
    ahora = ahora or datetime.now(timezone.utc)
    filas = []
    for v in cliente.uploads():
        for o in ocupadas_desde_youtube([v], canal.tz):
            if -3 <= (o - ahora).total_seconds() / 86400 <= dias:
                filas.append((o, v["status"].get("privacyStatus", "?"), v["snippet"].get("title", "")))
    return sorted(filas)
