"""Channels, slots and queue, with a fake YouTube: nothing touches the network."""
from __future__ import annotations

from datetime import datetime, timedelta, timezone

import pytest
import yaml

from subidor import canales
from subidor.cola import Parada, calendario, elementos, manifiesto, procesar
from conftest import FakeClient

LIMA = timezone(timedelta(hours=-5))
AHORA = datetime(2026, 11, 20, 12, 0, tzinfo=LIMA)


class Cliente(FakeClient):
    def __init__(self, ya=None, canal_id="UC_TEST", **kw):
        super().__init__(**kw)
        self.ya = ya or []
        self.canal_id = canal_id

    def my_channel(self):
        self.calls.append("channels.list")
        return {"id": self.canal_id, "title": "Canal de prueba"}

    def uploads(self, max_items=200):
        self.calls.append("uploads")
        return self.ya


@pytest.fixture
def canal(tmp_path, monkeypatch):
    monkeypatch.setenv("SUBIDOR_RAIZ", str(tmp_path))
    canales.crear("prueba")
    return canales.cargar("prueba")


def meter(canal, nombre, ficha=None, mini=False):
    (canal.cola / f"{nombre}.mp4").write_bytes(b"\x00\x00\x00\x18ftypmp42" + nombre.encode() + b"\x00" * 512)
    if ficha is not None:
        (canal.cola / f"{nombre}.yaml").write_text(yaml.safe_dump(ficha, allow_unicode=True), encoding="utf-8")
    if mini:
        (canal.cola / f"{nombre}.png").write_bytes(b"\x89PNG\r\n\x1a\n" + b"\x00" * 64)


def programado(fecha: datetime) -> dict:
    return {"id": "X", "snippet": {"title": "ya"}, "status": {"privacyStatus": "private",
            "publishAt": fecha.astimezone(timezone.utc).isoformat().replace("+00:00", "Z")}}


def test_plantilla_valida(canal):
    assert canal.horas == ["08:00"] and canal.dias_entre == 1 and canal.client_secret.name == "client_secret.json"
    assert canales.listar() == ["prueba"]


def test_ranuras_saltan_ocupadas_y_antelacion(canal):
    ocupada = datetime(2026, 11, 22, 8, 0, tzinfo=LIMA)
    r = canales.siguientes(canal, 3, [ocupada], ahora=AHORA)
    assert [x.day for x in r] == [21, 23, 24]       # the 20th at 08:00 has already passed; the 22nd is taken
    assert all(x.hour == 8 for x in r)


def test_ranuras_dia_si_dia_no(canal):
    c = canal.model_copy(update={"dias_entre": 2, "horas": ["08:00", "19:00"]})
    r = canales.siguientes(c, 4, [datetime(2026, 11, 21, 8, 0, tzinfo=LIMA)], ahora=AHORA)
    assert [(x.day, x.hour) for x in r] == [(21, 19), (23, 8), (23, 19), (25, 8)]


def test_ficha_en_espanol_y_valores_del_canal(canal):
    c = canal.model_copy(update={"etiquetas_base": ["canal"], "pie_descripcion": "Suscríbete", "contiene_ia": True})
    meter(c, "Mi Vídeo 1", {"titulo": "Hola", "descripcion": "Texto", "etiquetas": ["uno", "canal"]}, mini=True)
    el = elementos(c)[0]
    m = manifiesto(c, el, datetime(2026, 11, 21, 8, 0, tzinfo=LIMA))
    assert m.id == "mi-video-1" and m.title == "Hola" and m.tags == ["canal", "uno"]
    assert m.description.endswith("Suscríbete") and m.contains_synthetic_media is True
    assert m.privacy.value == "private" and m.publish_at.hour == 8 and m.thumbnail.name == "Mi Vídeo 1.png"


def test_plan_no_sube_nada(canal):
    meter(canal, "a", {"titulo": "A"})
    cli = Cliente()
    r = procesar(canal, cli, ejecutar=False, ahora=AHORA, aviso=lambda s: None)
    assert len(r.plan) == 1 and "videos.insert" not in cli.calls
    assert (canal.cola / "a.mp4").exists()


def test_programa_y_mueve_a_subidos(canal):
    meter(canal, "a", {"titulo": "A"})
    meter(canal, "b", {"titulo": "B"})
    meter(canal, "c")                                   # no sheet: error, the video stays in the queue
    cli = Cliente(ya=[programado(datetime(2026, 11, 21, 8, 0, tzinfo=LIMA))])
    r = procesar(canal, cli, ejecutar=True, ahora=AHORA, aviso=lambda s: None)
    assert [h["fecha"] for h in r.hechos] == ["2026-11-22 08:00", "2026-11-23 08:00"]
    assert cli.calls.count("videos.insert") == 2
    assert cli.last_body["status"]["privacyStatus"] == "private" and "publishAt" in cli.last_body["status"]
    assert len(r.errores) == 1 and (canal.cola / "c.mp4").exists()
    assert not (canal.cola / "a.mp4").exists() and len(list(canal.subidos.glob("*/informe.json"))) == 2


def test_canal_equivocado_se_detiene(canal):
    canal.archivo_id.write_text("UC_OTRO", encoding="utf-8")
    meter(canal, "a", {"titulo": "A"})
    with pytest.raises(Parada):
        procesar(canal, Cliente(), ejecutar=True, ahora=AHORA, aviso=lambda s: None)
    assert (canal.cola / "a.mp4").exists()


def test_publico_exige_permiso(canal):
    meter(canal, "a", {"titulo": "A", "privacidad": "public"})
    r = procesar(canal, Cliente(), ejecutar=True, ahora=AHORA, aviso=lambda s: None)
    assert r.errores and not r.hechos and (canal.cola / "a.mp4").exists()


def test_cuota_agotada_para_la_pasada(canal):
    meter(canal, "a", {"titulo": "A"})
    cli = Cliente(fail={"videos.insert": [RuntimeError("uploadLimitExceeded")]})
    with pytest.raises(Parada):
        procesar(canal, cli, ejecutar=True, ahora=AHORA, aviso=lambda s: None)


def test_calendario(canal):
    cli = Cliente(ya=[programado(datetime(2026, 11, 25, 8, 0, tzinfo=LIMA))])
    filas = calendario(canal, cli, ahora=AHORA)
    assert len(filas) == 1 and filas[0][1] == "private"


def test_mismo_archivo_dos_veces_no_se_duplica(canal):
    meter(canal, "a", {"titulo": "A"})
    (canal.cola / "a2.mp4").write_bytes((canal.cola / "a.mp4").read_bytes())
    (canal.cola / "a2.yaml").write_text("titulo: Otra\n", encoding="utf-8")
    cli = Cliente()
    r = procesar(canal, cli, ejecutar=True, ahora=AHORA, aviso=lambda s: None)
    assert cli.calls.count("videos.insert") == 1 and len(r.hechos) == 1
    assert len(r.errores) == 1 and (canal.cola / "a2.mp4").exists()
