#!/usr/bin/env python3
"""Conservatively eject the Phoenix admin UI foundation into an existing project."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import re
import sys
import tempfile

SKILL_DIR = Path(__file__).resolve().parent.parent
ASSETS = SKILL_DIR / "assets"
MODULE_RE = re.compile(r"^[A-Z][A-Za-z0-9]*(?:\.[A-Z][A-Za-z0-9]*)*$")
WEB_PATH_RE = re.compile(r"^[a-z][a-z0-9_]*$")


def non_empty(value: str) -> str:
    value = value.strip()
    if not value:
        raise argparse.ArgumentTypeError("must not be empty")
    return value


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--project", default=".")
    parser.add_argument("--web-module", required=True, type=non_empty, help="Example: MyAppWeb")
    parser.add_argument("--web-path", required=True, type=non_empty, help="Example: my_app_web")
    parser.add_argument("--brand", required=True, type=non_empty)
    parser.add_argument("--initials", required=True, type=non_empty)
    parser.add_argument("--with-dashboard", action="store_true")
    parser.add_argument(
        "--confirm-admin-in-phoenix-plan",
        action="store_true",
        help="Required confirmation that the approved plan explicitly places the product admin in Phoenix LiveView",
    )
    return parser.parse_args()


def elixir_string(value: str) -> str:
    return (
        value.replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("#{", "\\#{")
        .replace("\r", "\\r")
        .replace("\n", "\\n")
    )


def render(template: Path, replacements: dict[str, str]) -> str:
    content = template.read_text()
    for token, value in replacements.items():
        content = content.replace(token, value)
    return content


def main() -> int:
    args = parse_args()
    project = Path(args.project).resolve()

    if not args.confirm_admin_in_phoenix_plan:
        print(
            "error: refusing to scaffold without --confirm-admin-in-phoenix-plan; "
            "this skill is only for an admin explicitly approved for Phoenix LiveView",
            file=sys.stderr,
        )
        return 2

    if not (project / "mix.exs").is_file():
        print(f"error: {project} is not a Mix project", file=sys.stderr)
        return 2
    if not MODULE_RE.fullmatch(args.web_module):
        print("error: --web-module must be a valid Elixir alias", file=sys.stderr)
        return 2
    if not WEB_PATH_RE.fullmatch(args.web_path):
        print("error: --web-path must be one snake_case directory name", file=sys.stderr)
        return 2
    if len(args.initials) > 4:
        print("error: --initials must contain at most four characters", file=sys.stderr)
        return 2

    replacements = {
        "__WEB_MODULE__": args.web_module,
        "__BRAND__": elixir_string(args.brand),
        "__INITIALS__": elixir_string(args.initials.upper()),
    }

    destinations: list[tuple[Path, Path, bool]] = [
        (ASSETS / "admin-theme.css", project / "assets/css/admin-theme.css", False),
        (ASSETS / "admin_sidebar_hook.js", project / "assets/js/admin_sidebar_hook.js", False),
        (
            ASSETS / "admin_components.ex.eex",
            project / f"lib/{args.web_path}/components/admin_components.ex",
            True,
        ),
    ]

    if args.with_dashboard:
        destinations.append(
            (
                ASSETS / "dashboard_live.ex.eex",
                project / f"lib/{args.web_path}/live/admin/dashboard_live.ex",
                True,
            )
        )

    conflicts = [destination for _source, destination, _template in destinations if destination.exists()]
    if conflicts:
        print("error: refusing to overwrite existing files:", file=sys.stderr)
        for conflict in conflicts:
            print(f"  {conflict}", file=sys.stderr)
        print("Inspect and merge the skill assets manually.", file=sys.stderr)
        return 3

    staged: list[tuple[Path, Path]] = []
    created: list[Path] = []

    try:
        for source, destination, is_template in destinations:
            destination.parent.mkdir(parents=True, exist_ok=True)
            content = render(source, replacements).encode() if is_template else source.read_bytes()
            descriptor, temporary_name = tempfile.mkstemp(prefix=f".{destination.name}.", dir=destination.parent)
            temporary = Path(temporary_name)
            with os.fdopen(descriptor, "wb") as file:
                file.write(content)
                file.flush()
                os.fsync(file.fileno())
            staged.append((temporary, destination))

        for temporary, destination in staged:
            # Hard-linking within the destination directory is atomic and refuses
            # to overwrite a file created after the preflight conflict check.
            os.link(temporary, destination)
            temporary.unlink()
            created.append(destination)
            print(f"created {destination.relative_to(project)}")
    except Exception as error:
        for temporary, _destination in staged:
            temporary.unlink(missing_ok=True)
        for destination in created:
            destination.unlink(missing_ok=True)
        print(f"error: scaffold rolled back after failure: {error}", file=sys.stderr)
        return 4

    print("\nManual integration still required:")
    print('  1. import "./admin-theme.css" from assets/css/app.css')
    print("  2. register AdminSidebar in assets/js/app.js Hooks")
    print("  3. place admin routes in the authenticated/authorized live session")
    print("  4. replace placeholder dashboard data with authorized domain queries")
    print("  5. run compile, tests, assets and scripts/verify.sh")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
