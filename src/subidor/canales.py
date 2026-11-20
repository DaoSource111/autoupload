"""Channels: per-channel configuration (canal.yaml) and calculation of the next free publishing slots.

canal.yaml accepts English key names (see CLAVES_EN) as well as the original Spanish ones.
Use only one of the two forms for each key.
"""
from __future__ import annotations

import os
import re
from datetime import date, datetime, time, timedelta, timezone
from pathlib import Path
from typing import Iterable, Iterator, Optional

import yaml
from pydantic import BaseModel, Field, field_validator

NOMBRE = re.compile(r"^[a-z0-9][a-z0-9_-]{0,40}$")
PLANTILLA = """\
# Configuration for channel «{nombre}». See README.md, section "canal.yaml".
profile: {nombre}                # OAuth token name; created with: subidor auth {nombre} login
client_secret: ../../config/client_secret.json   # Google Cloud desktop OAuth credential (can be the same for every channel)
timezone: "-05:00"               # UTC offset for the hours below
hours: ["08:00"]                 # publishing hours for each day (one or several)
days_between: 1                  # 1 = every day · 2 = every other day · 7 = once a week
min_lead_hours: 3                # never schedule less than this many hours from now
max_per_run: 5                   # maximum videos per run (the API limits daily uploads)
language: es                     # language of the title, description and audio
category: "24"                   # 24 Entertainment · 27 Education · 28 Science & Technology · 22 People & Blogs
made_for_kids: false             # "made for kids" declaration (required by law)
contains_ai: null                # true if the video has realistic synthetic voice or imagery; null = do not declare
notify_subscribers: false        # notify subscribers when publishing
playlist: null                   # ID of a playlist (PL...) where each video is added
base_tags: []                    # tags added to every video
description_footer: ""           # text appended to the end of every description
"""

# English key names accepted in canal.yaml, mapped to the internal (Spanish) field names of Canal.
CLAVES_EN = {
    "profile": "perfil",
    "timezone": "zona_horaria",
    "hours": "horas",
    "days_between": "dias_entre",
    "min_lead_hours": "antelacion_min_horas",
    "max_per_run": "max_por_ejecucion",
    "language": "idioma",
    "category": "categoria",
    "made_for_kids": "para_ninos",
    "contains_ai": "contiene_ia",
    "notify_subscribers": "notificar",
    "playlist": "lista",
    "base_tags": "etiquetas_base",
    "description_footer": "pie_descripcion",
}


class Canal(BaseModel):
    nombre: str
    carpeta: Path
    perfil: str
    client_secret: Path
    zona_horaria: str = "-05:00"
    horas: list[str] = ["08:00"]
    dias_entre: int = Field(1, ge=1, le=30)
    antelacion_min_horas: float = Field(3, ge=0)
    max_por_ejecucion: int = Field(5, ge=1, le=50)
    idioma: str = "es"
    categoria: str = "24"
    para_ninos: bool = False
    contiene_ia: Optional[bool] = None
    notificar: bool = False
    lista: Optional[str] = None
    etiquetas_base: list[str] = []
    pie_descripcion: str = ""
    # Working folders, chosen when the channel is loaded (see _carpeta_de_trabajo).
    cola_dir: Optional[Path] = None
    subidos_dir: Optional[Path] = None

    @field_validator("zona_horaria")
    @classmethod
    def _tz(cls, v: str) -> str:
        if not re.match(r"^[+-]\d{2}:\d{2}$", v):
            raise ValueError("timezone (zona_horaria) must be an offset like -05:00 or +01:00")
        return v

    @field_validator("horas")
    @classmethod
    def _horas(cls, v: list[str]) -> list[str]:
        if not v:
            raise ValueError("hours (horas) cannot be empty")
        for h in v:
            if not re.match(r"^\d{2}:\d{2}$", h):
                raise ValueError(f"invalid hour: {h!r} (use HH:MM)")
        return sorted(v)

    @property
    def tz(self) -> timezone:
        signo = -1 if self.zona_horaria[0] == "-" else 1
        hh, mm = self.zona_horaria[1:].split(":")
        return timezone(signo * timedelta(hours=int(hh), minutes=int(mm)))

    @property
    def cola(self) -> Path:
        return self.cola_dir if self.cola_dir is not None else self.carpeta / "queue"

    @property
    def subidos(self) -> Path:
        return self.subidos_dir if self.subidos_dir is not None else self.carpeta / "uploaded"

    @property
    def estado(self) -> Path:
        return self.carpeta / "estado.sqlite"

    @property
    def archivo_id(self) -> Path:
        return self.carpeta / "canal_id.txt"

    def canal_id_esperado(self) -> Optional[str]:
        return self.archivo_id.read_text(encoding="utf-8").strip() if self.archivo_id.exists() else None


def raiz() -> Path:
    """Folder that holds canales/ and config/: SUBIDOR_RAIZ, or the project folder."""
    env = os.environ.get("SUBIDOR_RAIZ")
    return Path(env) if env else Path(__file__).resolve().parents[2]


def carpeta_canales(base: Optional[Path] = None) -> Path:
    return (base or raiz()) / "canales"


def listar(base: Optional[Path] = None) -> list[str]:
    c = carpeta_canales(base)
    return sorted(p.name for p in c.iterdir() if (p / "canal.yaml").exists()) if c.exists() else []


def _claves_canonicas(datos: dict, archivo: Path) -> dict:
    """Turns the English key names into the Spanish ones. Both forms of the same key is an error."""
    res = dict(datos)
    for en, es in CLAVES_EN.items():
        if en in res:
            if es in res:
                raise ValueError(f"{archivo}: «{es}» and «{en}» are the same option; keep only one of them")
            res[es] = res.pop(en)
    return res


def _carpeta_de_trabajo(carpeta: Path, antigua: str, nueva: str) -> Path:
    """Existing channels keep their Spanish folder (cola/, subidos/); channels without it use queue/ and uploaded/."""
    if (carpeta / antigua).exists():
        return carpeta / antigua
    return carpeta / nueva


def cargar(nombre: str, base: Optional[Path] = None) -> Canal:
    carpeta = carpeta_canales(base) / nombre
    archivo = carpeta / "canal.yaml"
    if not archivo.exists():
        raise FileNotFoundError(f"channel «{nombre}» does not exist ({archivo}). Create it with: subidor channel new {nombre}")
    datos = _claves_canonicas(yaml.safe_load(archivo.read_text(encoding="utf-8")) or {}, archivo)
    datos.setdefault("perfil", nombre)
    secret = Path(datos.get("client_secret", "../../config/client_secret.json"))
    datos["client_secret"] = secret if secret.is_absolute() else (carpeta / secret).resolve()
    datos["cola_dir"] = _carpeta_de_trabajo(carpeta, "cola", "queue")
    datos["subidos_dir"] = _carpeta_de_trabajo(carpeta, "subidos", "uploaded")
    return Canal(nombre=nombre, carpeta=carpeta, **datos)


def crear(nombre: str, base: Optional[Path] = None) -> Path:
    if not NOMBRE.match(nombre):
        raise ValueError("channel name may only contain lowercase letters, digits, - and _")
    carpeta = carpeta_canales(base) / nombre
    if (carpeta / "canal.yaml").exists():
        raise FileExistsError(f"channel «{nombre}» already exists")
    for sub in ("queue", "uploaded"):
        (carpeta / sub).mkdir(parents=True, exist_ok=True)
    (carpeta / "canal.yaml").write_text(PLANTILLA.format(nombre=nombre), encoding="utf-8")
    return carpeta


# --- dates ------------------------------------------------------------------------------------------

def ocupadas_desde_youtube(videos: Iterable[dict], tz: timezone) -> list[datetime]:
    """Dates already used on the channel: scheduled (publishAt) and published (publishedAt), in the channel's time zone."""
    res = []
    for v in videos:
        st, sn = v.get("status", {}), v.get("snippet", {})
        cuando = st.get("publishAt") if st.get("privacyStatus") == "private" else None
        if not cuando and st.get("privacyStatus") in ("public", "unlisted"):
            cuando = sn.get("publishedAt")
        if cuando:
            res.append(datetime.fromisoformat(cuando.replace("Z", "+00:00")).astimezone(tz))
    return res


def _ranuras(canal: Canal, desde: datetime) -> Iterator[datetime]:
    d = desde.astimezone(canal.tz).date()
    while True:
        for h in canal.horas:
            hh, mm = map(int, h.split(":"))
            yield datetime.combine(d, time(hh, mm), tzinfo=canal.tz)
        d += timedelta(days=1)


def siguientes(canal: Canal, n: int, ocupadas: Iterable[datetime], ahora: Optional[datetime] = None) -> list[datetime]:
    """The next n free slots.

    A slot is free if nobody already uses that same time on that day and, with days_between > 1,
    there is no upload less than days_between days away (another time on the same day is fine).
    """
    ahora = ahora or datetime.now(timezone.utc)
    minimo = ahora + timedelta(hours=canal.antelacion_min_horas)
    usadas = {o.astimezone(canal.tz).replace(second=0, microsecond=0) for o in ocupadas}
    dias: set[date] = {u.date() for u in usadas}
    res: list[datetime] = []
    for r in _ranuras(canal, minimo):
        if len(res) >= n:
            break
        if r <= minimo or r in usadas:
            continue
        if canal.dias_entre > 1 and any(0 < abs((r.date() - d).days) < canal.dias_entre for d in dias):
            continue
        res.append(r)
        usadas.add(r)
        dias.add(r.date())
    return res
