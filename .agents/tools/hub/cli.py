"""Command-line entry: python .agents/tools/hub <command> [options]"""
import argparse
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent


def project_root(arg=None) -> Path:
    """--root, else the project this tool is installed in, else the nearest
    ancestor of the cwd holding .agents/project.json, else the cwd."""
    if arg:
        return Path(arg).resolve()
    installed = HERE.parents[2]
    if (installed / ".agents" / "project.json").exists():
        return installed
    cur = Path.cwd().resolve()
    for d in (cur, *cur.parents):
        if (d / ".agents" / "project.json").exists():
            return d
    return cur


def _list(s):
    return [x.strip() for x in s.split(",") if x.strip()] if s else []


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(prog="hub", description="hub: reusable AI development framework")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("install", help="install hub into a project (never overwrites)")
    p.add_argument("target", nargs="?", default=".")
    p.add_argument("--source", help="path to a hub checkout (default: the checkout this tool runs from)")
    p.add_argument("--platforms", help="comma list: claude-code,codex,copilot,cursor")
    p.add_argument("--packs", help="comma list of packs, e.g. python,laravel")
    p.add_argument("--from-template", action="store_true", help="first run in a repo made with 'Use this template'")
    p.add_argument("--claude-import", action="store_true", help="prepend @AGENTS.md to an existing CLAUDE.md")
    p.add_argument("--yes", "-y", action="store_true", help="don't ask for confirmation")
    p.add_argument("--dry-run", action="store_true")

    p = sub.add_parser("sync", help="generate platform adapters from .agents/ and refresh the AGENTS.md hub block")
    p.add_argument("--check", action="store_true", help="exit 1 if generated files are out of date")
    p.add_argument("--force", action="store_true", help="overwrite hand-edited generated files")
    p.add_argument("--root")

    p = sub.add_parser("validate", help="check structure, budgets, links, schemas and drift")
    p.add_argument("--template", action="store_true", help="validate a hub template checkout")
    p.add_argument("--root")

    p = sub.add_parser("update", help="upgrade framework files from a newer hub checkout")
    p.add_argument("--source", required=True, help="path to the newer hub checkout (a git clone)")
    p.add_argument("--dry-run", action="store_true")
    p.add_argument("--root")

    p = sub.add_parser("detect", help="print repository facts as JSON")
    p.add_argument("--root")

    p = sub.add_parser("tasks", help="check a multi-agent task board")
    p.add_argument("action", choices=["check"])
    p.add_argument("file", nargs="?", default=".agents/state/tasks.md")

    p = sub.add_parser("context", help="print the session handoff (used by the SessionStart hook)")
    p.add_argument("--root")

    sub.add_parser("version", help="print the installed hub version")

    a = ap.parse_args(argv)
    if a.cmd == "install":
        import install
        return install.run(a.target, source=a.source, platforms=_list(a.platforms) or None,
                           packs=_list(a.packs), from_template=a.from_template, yes=a.yes,
                           dry_run=a.dry_run, claude_import=a.claude_import)
    if a.cmd == "sync":
        import sync
        return sync.run(project_root(a.root), check=a.check, force=a.force)
    if a.cmd == "validate":
        import validate
        return validate.run(project_root(a.root) if not a.template else Path(a.root or ".").resolve(),
                            template=a.template)
    if a.cmd == "update":
        import update
        return update.run(project_root(a.root), Path(a.source).resolve(), dry_run=a.dry_run)
    if a.cmd == "detect":
        import detect
        print(json.dumps(detect.scan(project_root(a.root)), indent=2))
        return 0
    if a.cmd == "tasks":
        import tasks
        return tasks.run_check(Path(a.file))
    if a.cmd == "context":
        import context
        return context.run(project_root(a.root))
    if a.cmd == "version":
        import lock
        print(lock.Lock.load(project_root()).version or "not installed")
        return 0
    return 2


if __name__ == "__main__":
    sys.exit(main())
