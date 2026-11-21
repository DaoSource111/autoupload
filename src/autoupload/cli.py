"""CLI: `autoupload <command>`. Nothing is uploaded without --yes; nothing is made public without --allow-public."""
from __future__ import annotations

import argparse
import json
import logging
import sys
from pathlib import Path

from . import __version__
from .fsutil import write_atomic

OK, INVALID, AUTH, REMOTE = 0, 2, 3, 4


def _logging(verbose: bool) -> None:
    logging.basicConfig(level=logging.DEBUG if verbose else logging.WARNING, format="%(levelname)s %(name)s: %(message)s", force=True)


def _client(channel):
    from . import auth
    from .youtube_client import YouTubeClient
    return YouTubeClient.from_credentials(auth.load(channel.profile))


def build_parser() -> argparse.ArgumentParser:
    ap = argparse.ArgumentParser(prog="autoupload", description="Upload and schedule videos on your YouTube channels (official API)")
    ap.add_argument("--version", action="version", version=__version__)
    ap.add_argument("-v", "--verbose", action="store_true")
    sub = ap.add_subparsers(dest="cmd", required=True)

    c = sub.add_parser("channel", help="create a channel: autoupload channel new <name>")
    c.add_argument("action", choices=["new"])
    c.add_argument("name")
    sub.add_parser("channels", help="list the configured channels and their queue")
    sub.add_parser("doctor", help="check the installation offline and say how to fix each problem")
    mg = sub.add_parser("migrate", help="move data left inside the venv by version 1.1.0 to the data folder")
    mg.add_argument("--yes", action="store_true", help="move the files (without it, only list what would move)")

    a = sub.add_parser("auth", help="connect a channel: autoupload auth <channel> login | whoami | logout")
    a.add_argument("channel")
    a.add_argument("action", choices=["login", "whoami", "logout"])

    k = sub.add_parser("calendar", help="scheduled and recent uploads of a channel (read only)")
    k.add_argument("channel")
    k.add_argument("--days", type=int, default=30)

    p = sub.add_parser("schedule", help="upload the queue as private with a date (without --yes it only shows the plan)")
    p.add_argument("channel", nargs="?")
    p.add_argument("--all", dest="all", action="store_true", help="all configured channels")
    p.add_argument("--yes", action="store_true", help="confirm that you want to upload to your channel")
    p.add_argument("--max", type=int, help="maximum number of videos per channel in this run")
    p.add_argument("--allow-public", dest="allow_public", action="store_true", help="allow sheets with privacy: public")

    v = sub.add_parser("validate", help="check a standalone manifest, no network")
    v.add_argument("manifest", type=Path)
    pl = sub.add_parser("plan", help="plan for a standalone manifest")
    pl.add_argument("manifest", type=Path)
    q = sub.add_parser("publish", help="upload a standalone manifest to a channel (requires --yes)")
    q.add_argument("manifest", type=Path)
    q.add_argument("--channel", dest="channel", required=True)
    q.add_argument("--yes", action="store_true")
    q.add_argument("--allow-public", dest="allow_public", action="store_true")

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
    return build_parser().parse_args(argv)


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
    except (FileNotFoundError, ValueError) as e:  # bad channel name, bad channel.yaml, ...
        if args.verbose:
            raise
        print(f"Error: {e}", file=sys.stderr)
        return INVALID


def _run(args) -> int:
    from . import auth, channels
    from .queue import StopRun, calendar_rows, process_queue, queue_items

    if args.cmd == "channel":
        try:
            folder = channels.create_channel(args.name)
        except (ValueError, FileExistsError) as e:
            print(e, file=sys.stderr)
            return INVALID
        print(f"Channel created in {folder}\n1) Review {folder / 'channel.yaml'}\n2) Connect it: autoupload auth {args.name} login")
        return OK

    if args.cmd == "migrate":
        actions = channels.migrate_legacy(execute=args.yes)
        if not actions:
            print(f"Nothing to migrate. Data folder: {channels.root()}")
            return OK
        print("\n".join(actions))
        print("Done." if args.yes else "Nothing was moved. Run again with --yes to move these files.")
        return OK

    if args.cmd == "doctor":
        from . import doctor
        checks = doctor.run()
        print(doctor.render(checks))
        return INVALID if any(c.level == doctor.ERROR for c in checks) else OK

    if args.cmd == "channels":
        names = channels.list_channels()
        if not names:
            print("No channels. Create one with: autoupload channel new <name>")
        for n in names:
            channel = channels.load_channel(n)
            connected = "connected" if auth.token_path(channel.profile).exists() else "NOT CONNECTED"
            print(f"{n:<16} queue: {len(queue_items(channel)):>3}   {connected}   {', '.join(channel.hours)} every {channel.days_between} day(s) ({channel.timezone})")
        return OK

    if args.cmd == "auth":
        channel = channels.load_channel(args.channel)
        try:
            if args.action == "login":
                auth.login(channel.client_secret, channel.profile)
                yt = _client(channel).my_channel()
                previous = channel.expected_channel_id()
                if previous and previous != yt["id"]:
                    print(f"Note: this channel used to be {previous}, and you have now signed in as {yt['title']} ({yt['id']}).")
                write_atomic(channel.channel_id_file, yt["id"])
                print(f"Connected '{args.channel}' → {yt['title']} ({yt['id']})")
            elif args.action == "logout":
                print("Signed out." if auth.logout(channel.profile) else "There was no session.")
            else:
                yt = _client(channel).my_channel()
                print(f"'{args.channel}' → {yt['title']} ({yt['id']})")
        except auth.AuthError as e:
            print("Authentication:", e, file=sys.stderr)
            return AUTH
        return OK

    if args.cmd == "calendar":
        channel = channels.load_channel(args.channel)
        try:
            rows = calendar_rows(channel, _client(channel), args.days)
        except auth.AuthError as e:
            print("Authentication:", e, file=sys.stderr)
            return AUTH
        for when, privacy, title in rows:
            print(f"{when:%Y-%m-%d %H:%M}  {'scheduled' if privacy == 'private' else privacy:<10}  {title}")
        print(f"{len(rows)} uploads (time {channel.timezone}). Local queue: {len(queue_items(channel))} video(s).")
        return OK

    if args.cmd == "schedule":
        names = channels.list_channels() if args.all else ([args.channel] if args.channel else [])
        if not names:
            print("Give a channel or use --all.", file=sys.stderr)
            return INVALID
        code = OK
        for n in names:
            channel = channels.load_channel(n)
            if not queue_items(channel):
                print(f"[{n}] queue empty")
                continue
            try:
                r = process_queue(channel, _client(channel), execute=args.yes, limit=args.max, allow_public=args.allow_public,
                                  progress=lambda x: print(f"\r    uploading… {x * 100:5.1f}%", end="", flush=True))
            except auth.AuthError as e:
                print(f"[{n}] Authentication: {e}", file=sys.stderr)
                code = AUTH
                continue
            except StopRun as e:
                print(e, file=sys.stderr)
                code = REMOTE
                continue
            if r.errors:
                code = code or INVALID
            if not args.yes and r.plan:
                print(f"[{n}] Nothing was uploaded. Run again with --yes to execute this plan.")
        return code

    from .manifest import load_manifest
    try:
        m = load_manifest(args.manifest)
    except Exception as e:  # noqa: BLE001
        print("Invalid manifest:", e, file=sys.stderr)
        return INVALID

    if args.cmd == "validate":
        from .validators import validate
        rep = validate(m)
        for e in rep.errors:
            print("ERROR:", e)
        for w in rep.warnings:
            print("warning:", w)
        print("OK" if rep.ok else "NOT VALID", json.dumps(rep.facts, ensure_ascii=False))
        return OK if rep.ok else INVALID

    if args.cmd == "plan":
        from .publisher import make_plan
        pl = make_plan(m)
        print(pl.render())
        return OK if pl.report.ok else INVALID

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
        channel = channels.load_channel(args.channel)
        print(make_plan(m).render())
        if not args.yes:
            print("\nNothing has been uploaded. Run again with --yes to execute this plan.")
            return OK
        try:
            cli = _client(channel)
            expected, real = channel.expected_channel_id(), cli.my_channel()
            if expected and real["id"] != expected:
                print(f"Blocked: the token belongs to {real['title']} ({real['id']}), not to {expected}", file=sys.stderr)
                return AUTH
            res = publish(m, cli, State(channel.state_db), confirm=True, allow_public=args.allow_public,
                          on_progress=lambda x: print(f"\ruploading… {x * 100:5.1f}%", end="", flush=True))
        except SafetyError as e:
            print("\nBlocked:", e, file=sys.stderr)
            return INVALID
        except auth.AuthError as e:
            print("\nAuthentication:", e, file=sys.stderr)
            return AUTH
        except Exception as e:  # noqa: BLE001
            print("\nRemote error (the local state lets you resume):", e, file=sys.stderr)
            return REMOTE
        print(f"\nVideo: {res['url']}\nReport: {write_report(res, args.manifest.parent / f'report-{m.id}.json')}")
        return OK
    return INVALID


if __name__ == "__main__":
    raise SystemExit(main())
