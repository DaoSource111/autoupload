"""canal.yaml: English key names, Spanish key names, duplicate-key error, and working folders (cola/ or queue/)."""
from __future__ import annotations

from pathlib import Path

import pytest

from subidor import canales

INGLES = """\
profile: prueba-en
timezone: "+01:00"
hours: ["19:30", "08:00"]
days_between: 2
min_lead_hours: 1
max_per_run: 3
language: en
category: "28"
made_for_kids: false
contains_ai: true
notify_subscribers: false
playlist: PL123
base_tags: ["ai"]
description_footer: "Thanks"
"""


def escribir(base: Path, nombre: str, texto: str) -> Path:
    carpeta = base / "canales" / nombre
    carpeta.mkdir(parents=True)
    (carpeta / "canal.yaml").write_text(texto, encoding="utf-8")
    return carpeta


def test_claves_en_ingles_se_leen(tmp_path):
    escribir(tmp_path, "ing", INGLES)
    c = canales.cargar("ing", base=tmp_path)
    assert c.perfil == "prueba-en" and c.zona_horaria == "+01:00"
    assert c.horas == ["08:00", "19:30"] and c.dias_entre == 2 and c.antelacion_min_horas == 1
    assert c.max_por_ejecucion == 3 and c.idioma == "en" and c.categoria == "28"
    assert c.contiene_ia is True and c.notificar is False and c.para_ninos is False
    assert c.lista == "PL123" and c.etiquetas_base == ["ai"] and c.pie_descripcion == "Thanks"


def test_claves_en_espanol_siguen_funcionando(tmp_path):
    escribir(tmp_path, "esp", 'perfil: esp\nhoras: ["19:00"]\ncontiene_ia: false\n')
    c = canales.cargar("esp", base=tmp_path)
    assert c.perfil == "esp" and c.horas == ["19:00"] and c.contiene_ia is False


@pytest.mark.parametrize("texto, par", [
    ('horas: ["08:00"]\nhours: ["19:00"]\n', ("horas", "hours")),
    ("idioma: es\nlanguage: en\n", ("idioma", "language")),
    ("perfil: a\nprofile: b\n", ("perfil", "profile")),
])
def test_clave_duplicada_da_error_claro(tmp_path, texto, par):
    escribir(tmp_path, "dup", texto)
    with pytest.raises(ValueError) as error:
        canales.cargar("dup", base=tmp_path)
    assert par[0] in str(error.value) and par[1] in str(error.value) and "keep only one" in str(error.value)


def test_plantilla_nueva_usa_claves_en_ingles(tmp_path):
    carpeta = canales.crear("nuevo", base=tmp_path)
    texto = (carpeta / "canal.yaml").read_text(encoding="utf-8")
    assert "profile: nuevo" in texto and "hours:" in texto
    assert "horas:" not in texto and "perfil:" not in texto
    c = canales.cargar("nuevo", base=tmp_path)
    assert c.perfil == "nuevo" and c.horas == ["08:00"] and c.dias_entre == 1 and c.idioma == "es"


def test_canal_existente_con_cola_y_subidos_usa_esas_carpetas(tmp_path):
    carpeta = escribir(tmp_path, "viejo", 'horas: ["08:00"]\n')
    (carpeta / "cola").mkdir()
    (carpeta / "subidos").mkdir()
    c = canales.cargar("viejo", base=tmp_path)
    assert c.cola == carpeta / "cola" and c.subidos == carpeta / "subidos"


def test_canal_nuevo_crea_y_usa_queue_y_uploaded(tmp_path):
    canales.crear("nuevo", base=tmp_path)
    c = canales.cargar("nuevo", base=tmp_path)
    assert c.cola == tmp_path / "canales" / "nuevo" / "queue" and c.subidos == tmp_path / "canales" / "nuevo" / "uploaded"
    assert c.cola.is_dir() and c.subidos.is_dir()


def test_sin_carpeta_cola_se_usa_queue(tmp_path):
    escribir(tmp_path, "sin", 'horas: ["08:00"]\n')
    c = canales.cargar("sin", base=tmp_path)
    assert c.cola.name == "queue" and c.subidos.name == "uploaded"


def test_con_cola_y_queue_gana_cola(tmp_path):
    carpeta = escribir(tmp_path, "mixto", 'horas: ["08:00"]\n')
    (carpeta / "cola").mkdir()
    (carpeta / "queue").mkdir()
    c = canales.cargar("mixto", base=tmp_path)
    assert c.cola.name == "cola"
