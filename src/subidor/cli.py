"""CLI: `subidor <command>`. Nothing is uploaded without --yes; nothing is made public without --allow-public.

The Spanish command and option names (canal, canales, calendario, programar, validar, publicar,
--todos, --dias, --canal, --permitir-publico) still work as aliases.
"""
from __future__ import annotations

import argparse
import json
import logging
import sys
from pathlib import Path

from . import __version__

OK, INVALIDO, AUTH, REMOTO = 0, 2, 3, 4

# Spanish command names kept as aliases; each one is mapped to the English name used in main().
COMMAND_ALIASES = {"canal": "channel", "canales": "channels", "calendario": "calendar",
                   "programar": "schedule", "validar": "validate", "publicar": "publish"}
CHANNEL_ACTIONS = {"nuevo": "new"}


def _logging(verbose: bool) -> None:
    logging.basicConfig(level=logging.DEBUG if verbose else logging.WARNING, format="%(levelname)s %(name)s: %(message)s", force=True)


def _cliente(canal):
    from . import auth
    from .youtube_client import YouTubeClient
    return YouTubeClient.from_credentials(auth.load(canal.perfil))


def build_parser() -> argparse.ArgumentParser:
    ap = argparse.ArgumentParser(prog="subidor", description="Upload and schedule videos on your YouTube channels (official API)")
    ap.add_argument("--version", action="version", version=__version__)
    ap.add_argument("-v", "--verbose", action="store_true")
    sub = ap.add_subparsers(dest="cmd", required=True)

    c = sub.add_parser("channel", aliases=["canal"], help="create a channel: subidor channel new <name>")
    c.add_argument("action", choices=["new", "nuevo"])
    c.add_argument("name")
    sub.add_parser("channels", aliases=["canales"], help="list the configured channels and their queue")

    a = sub.add_parser("auth", help="connect a channel: subidor auth <channel> login | whoami | logout")
    a.add_argument("channel")
    a.add_argument("action", choices=["login", "whoami", "logout"])

    k = sub.add_parser("calendar", aliases=["calendario"], help="scheduled and recent uploads of a channel (read only)")
    k.add_argument("channel")
    k.add_argument("--days", "--dias", dest="days", type=int, default=30)

    p = sub.add_parser("schedule", aliases=["programar"], help="upload the queue as private with a date (without --yes it only shows the plan)")
    p.add_argument("channel", nargs="?")
    p.add_argument("--all", "--todos", dest="all", action="store_true", help="all configured channels")
    p.add_argument("--yes", action="store_true", help="confirm that you want to upload to your channel")
    p.add_argument("--max", type=int, help="maximum number of videos per channel in this run")
    p.add_argument("--allow-public", "--permitir-publico", dest="allow_public", action="store_true", help="allow sheets with privacy: public")

    v = sub.add_parser("validate", aliases=["validar"], help="check a standalone manifest, no network")
    v.add_argument("manifest", type=Path)
    pl = sub.add_parser("plan", help="plan for a standalone manifest")
    pl.add_argument("manifest", type=Path)
    q = sub.add_parser("publish", aliases=["publicar"], help="upload a standalone manifest to a channel (requires --yes)")
    q.add_argument("manifest", type=Path)
    q.add_argument("--channel", "--canal", dest="channel", required=True)
    q.add_argument("--yes", action="store_true")
    q.add_argument("--allow-public", "--permitir-publico", dest="allow_public", action="store_true")

    s = sub.add_parser("studio-package", help="prepare dubbing and thumbnails per language for Studio")
    s.add_argument("manifest", type=Path)
    s.add_argument("--video-id")
    s.add_argument("--out", type=Path)
    d = sub.add_parser("studio-done", help="mark a language as uploaded in Studio")
    d.add_argument("manifest", type=Path)
    d.add_argument("--language", required=True)
    d.add_argument("--kind", choices=["audio_tracks", "localized_thumbnails"], default="audio_tracks")
    return ap


def parse_args(argv=None) -> argparse.Namespace:
    """Parses the command line and maps Spanish aliases to the English names."""
    args = build_parser().parse_args(argv)
    args.cmd = COMMAND_ALIASES.get(args.cmd, args.cmd)
    if args.cmd == "channel":
        args.action = CHANNEL_ACTIONS.get(args.action, args.action)
    return args


def main(argv=None) -> int:
    for s in (sys.stdout, sys.stderr):
        try:
            s.reconfigure(encoding="utf-8")
        except (AttributeError, ValueError):
            pass
    args = parse_args(argv)
    _logging(args.verbose)
    try:
        return _run(args)
    except (FileNotFoundError, ValueError) as e:  # bad channel name, bad canal.yaml, ...
        if args.verbose:
            raise
        print(f"Error: {e}", file=sys.stderr)
        return INVALIDO


def _run(args) -> int:
    from . import auth, canales
    from .cola import Parada, calendario, elementos, procesar

    if args.cmd == "channel":
        try:
            carpeta = canales.crear(args.name)
        except (ValueError, FileExistsError) as e:
            print(e, file=sys.stderr)
            return INVALIDO
        print(f"Channel created in {carpeta}\n1) Review {carpeta / 'canal.yaml'}\n2) Connect it: subidor auth {args.name} login")
        return OK

    if args.cmd == "channels":
        nombres = canales.listar()
        if not nombres:
            print("No channels. Create one with: subidor channel new <name>")
        for n in nombres:
            ca = canales.cargar(n)
            conectado = "connected" if auth.token_path(ca.perfil).exists() else "NOT CONNECTED"
            print(f"{n:<16} queue: {len(elementos(ca)):>3}   {conectado}   {', '.join(ca.horas)} every {ca.dias_entre} day(s) ({ca.zona_horaria})")
        return OK

    if args.cmd == "auth":
        ca = canales.cargar(args.channel)
        try:
            if args.action == "login":
                auth.login(ca.client_secret, ca.perfil)
                ch = _cliente(ca).my_channel()
                previo = ca.canal_id_esperado()
                if previo and previo != ch["id"]:
                    print(f"Note: this channel used to be {previo}, and you have now signed in as {ch['title']} ({ch['id']}).")
                ca.archivo_id.write_text(ch["id"], encoding="utf-8")
                print(f"Connected «{args.channel}» → {ch['title']} ({ch['id']})")
            elif args.action == "logout":
                print("Signed out." if auth.logout(ca.perfil) else "There was no session.")
            else:
                ch = _cliente(ca).my_channel()
                print(f"«{args.channel}» → {ch['title']} ({ch['id']})")
        except auth.AuthError as e:
            print("Authentication:", e, file=sys.stderr)
            return AUTH
        return OK

    if args.cmd == "calendar":
        ca = canales.cargar(args.channel)
        try:
            filas = calendario(ca, _cliente(ca), args.days)
        except auth.AuthError as e:
            print("Authentication:", e, file=sys.stderr)
            return AUTH
        for cuando, priv, titulo in filas:
            print(f"{cuando:%Y-%m-%d %H:%M}  {'scheduled' if priv == 'private' else priv:<10}  {titulo}")
        print(f"{len(filas)} uploads (time {ca.zona_horaria}). Local queue: {len(elementos(ca))} video(s).")
        return OK

    if args.cmd == "schedule":
        nombres = canales.listar() if args.all else ([args.channel] if args.channel else [])
        if not nombres:
            print("Give a channel or use --all.", file=sys.stderr)
            return INVALIDO
        codigo = OK
        for n in nombres:
            ca = canales.cargar(n)
            if not elementos(ca):
                print(f"[{n}] queue empty")
                continue
            try:
                r = procesar(ca, _cliente(ca), ejecutar=args.yes, maximo=args.max, permitir_publico=args.allow_public,
                             progreso=lambda x: print(f"\r    uploading… {x * 100:5.1f}%", end="", flush=True))
            except auth.AuthError as e:
                print(f"[{n}] Authentication: {e}", file=sys.stderr)
                codigo = AUTH
                continue
            except Parada as e:
                print(e, file=sys.stderr)
                codigo = REMOTO
                continue
            if r.errores:
                codigo = codigo or INVALIDO
            if not args.yes and r.plan:
                print(f"[{n}] Nothing was uploaded. Run again with --yes to execute this plan.")
        return codigo

    from .manifest import load_manifest
    try:
        m = load_manifest(args.manifest)
    except Exception as e:  # noqa: BLE001
        print("Invalid manifest:", e, file=sys.stderr)
        return INVALIDO

    if args.cmd == "validate":
        from .validators import validate
        rep = validate(m)
        for e in rep.errors:
            print("ERROR:", e)
        for w in rep.warnings:
            print("warning:", w)
        print("OK" if rep.ok else "NOT VALID", json.dumps(rep.facts, ensure_ascii=False))
        return OK if rep.ok else INVALIDO

    if args.cmd == "plan":
        from .publisher import make_plan
        pl = make_plan(m)
        print(pl.render())
        return OK if pl.report.ok else INVALIDO

    if args.cmd == "studio-package":
        from .studio_package import build
        print("Studio package in", build(m, args.out or args.manifest.parent / f"studio-{m.id}", args.video_id))
        return OK

    if args.cmd == "studio-done":
        from .studio_package import mark_done
        mark_done(args.manifest.parent / f"studio-{m.id}", args.language, args.kind)
        print(f"{args.language}: marked as uploaded in Studio (check it in the player)")
        return OK

    if args.cmd == "publish":
        from .publisher import SafetyError, make_plan, publish, write_report
        from .state import State
        ca = canales.cargar(args.channel)
        print(make_plan(m).render())
        if not args.yes:
            print("\nNothing has been uploaded. Run again with --yes to execute this plan.")
            return OK
        try:
            cli = _cliente(ca)
            esperado, real = ca.canal_id_esperado(), cli.my_channel()
            if esperado and real["id"] != esperado:
                print(f"Blocked: the token belongs to {real['title']} ({real['id']}), not to {esperado}", file=sys.stderr)
                return AUTH
            res = publish(m, cli, State(ca.estado), confirm=True, allow_public=args.allow_public,
                          on_progress=lambda x: print(f"\ruploading… {x * 100:5.1f}%", end="", flush=True))
        except SafetyError as e:
            print("\nBlocked:", e, file=sys.stderr)
            return INVALIDO
        except auth.AuthError as e:
            print("\nAuthentication:", e, file=sys.stderr)
            return AUTH
        except Exception as e:  # noqa: BLE001
            print("\nRemote error (the local state lets you resume):", e, file=sys.stderr)
            return REMOTO
        print(f"\nVideo: {res['url']}\nReport: {write_report(res, args.manifest.parent / f'informe-{m.id}.json')}")
        return OK
    return INVALIDO


if __name__ == "__main__":
    raise SystemExit(main())
