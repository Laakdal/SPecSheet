"""hub detect: list facts about a repository for the init-project skill.

It reports evidence (which files exist, which dependencies they name). It
does not decide what the project is; the agent does that, with the user.
"""
import json
import re
from pathlib import Path

import fsutil

SKIP = {".git", "node_modules", "vendor", "target", "dist", "build", ".venv", "venv",
        "__pycache__", ".agents", ".next", ".nuxt", ".gradle", ".idea", ".vscode", "Pods",
        ".terraform", "coverage", ".tox", ".mypy_cache", ".pytest_cache"}
MAX_DEPTH = 3

# manifest filename -> ecosystem kind
MANIFESTS = {
    "package.json": "node", "composer.json": "php", "Cargo.toml": "rust",
    "pyproject.toml": "python", "requirements.txt": "python", "setup.py": "python",
    "Pipfile": "python", "environment.yml": "python-conda", "go.mod": "go",
    "pom.xml": "java-maven", "build.gradle": "jvm-gradle", "build.gradle.kts": "jvm-gradle",
    "Gemfile": "ruby", "pubspec.yaml": "dart", "Package.swift": "swift",
    "mix.exs": "elixir", "deno.json": "deno", "CMakeLists.txt": "cmake", "Makefile": "make",
    "justfile": "just", "Chart.yaml": "helm",
}
SUFFIX_MANIFESTS = {".csproj": "dotnet", ".sln": "dotnet", ".tf": "terraform", ".ipynb": "notebook"}
LOCKFILES = {"package-lock.json", "pnpm-lock.yaml", "yarn.lock", "bun.lockb", "bun.lock",
             "composer.lock", "Cargo.lock", "poetry.lock", "uv.lock", "Pipfile.lock",
             "go.sum", "Gemfile.lock", "pubspec.lock"}
INSTRUCTION_FILES = ["AGENTS.md", "AGENTS.override.md", "CLAUDE.md", "CLAUDE.local.md", ".claude/CLAUDE.md",
                     "GEMINI.md", ".cursorrules", ".github/copilot-instructions.md"]
INSTRUCTION_DIRS = [".claude/rules", ".claude/agents", ".claude/skills", ".claude/commands",
                    ".cursor/rules", ".cursor/agents", ".github/instructions", ".github/agents",
                    ".github/prompts", ".codex", ".agents/skills"]

# dependency name -> hint (matched against dependency keys / requirement names)
HINTS = {
    "next": "nextjs", "react": "react", "react-dom": "react", "vue": "vue", "nuxt": "nuxt",
    "svelte": "svelte", "@sveltejs/kit": "sveltekit", "@angular/core": "angular",
    "express": "express", "fastify": "fastify", "@nestjs/core": "nestjs", "hono": "hono",
    "electron": "electron", "@tauri-apps/api": "tauri", "react-native": "react-native",
    "expo": "expo", "vite": "vite", "typescript": "typescript", "prisma": "prisma",
    "@prisma/client": "prisma", "drizzle-orm": "drizzle", "jest": "jest", "vitest": "vitest",
    "@playwright/test": "playwright", "tailwindcss": "tailwind",
    "laravel/framework": "laravel", "symfony/framework-bundle": "symfony",
    "phpunit/phpunit": "phpunit", "pestphp/pest": "pest", "inertiajs/inertia-laravel": "inertia",
    "tauri": "tauri", "tokio": "tokio", "axum": "axum", "actix-web": "actix", "bevy": "bevy",
    "egui": "egui", "clap": "clap", "serde": "serde", "sqlx": "sqlx", "diesel": "diesel",
    "django": "django", "flask": "flask", "fastapi": "fastapi", "sqlalchemy": "sqlalchemy",
    "torch": "pytorch", "tensorflow": "tensorflow", "scikit-learn": "scikit-learn",
    "pandas": "pandas", "numpy": "numpy", "jupyter": "jupyter", "notebook": "jupyter",
    "pyspark": "spark", "apache-airflow": "airflow", "dbt-core": "dbt", "pytest": "pytest",
    "typer": "typer", "click": "click", "langchain": "langchain", "transformers": "transformers",
    "spring-boot": "spring-boot", "org.springframework.boot": "spring-boot",
    "rails": "rails", "flutter": "flutter",
}


def _norm_dep(name: str) -> str:
    return re.split(r"[\[<>=~!; ]", name.strip().lower(), maxsplit=1)[0]


def _hints_for(names):
    return sorted({HINTS[n] for n in (_norm_dep(x) for x in names) if n in HINTS})


def _deps(path: Path, kind: str):
    try:
        text = fsutil.read_text(path)
    except (OSError, UnicodeDecodeError):
        return []
    name = path.name
    try:
        if name in ("package.json", "composer.json"):
            data = json.loads(text)
            keys = ["dependencies", "devDependencies", "peerDependencies", "require", "require-dev"]
            return [k for sect in keys for k in (data.get(sect) or {})]
        if name == "requirements.txt":
            return [ln for ln in text.splitlines() if ln.strip() and not ln.startswith(("#", "-"))]
        if name in ("pyproject.toml", "Cargo.toml", "Pipfile"):
            return re.findall(r'^\s*"?([A-Za-z0-9_.\-]+)"?\s*=', text, re.M) + \
                re.findall(r'"([A-Za-z0-9_.\-]+)\s*(?:[<>=~!\[;][^"]*)?"', text)
        if name in ("pom.xml",):
            return re.findall(r"<artifactId>([^<]+)</artifactId>", text) + \
                re.findall(r"<groupId>([^<]+)</groupId>", text)
        if name.startswith("build.gradle"):
            return [g for pair in re.findall(r"['\"]([\w.\-]+):([\w.\-]+)", text) for g in pair]
        if name == "Gemfile":
            return re.findall(r"^\s*gem\s+['\"]([^'\"]+)", text, re.M)
        if name == "pubspec.yaml":
            return ["flutter"] if re.search(r"^\s*flutter:", text, re.M) else []
    except (ValueError, AttributeError):
        return []
    return []


DOC_DIRS = ("", "docs", "doc")
DECISION_DIRS = {"decisions", "adr", "adrs", "decision-records"}


def doc_files(root) -> dict:
    """Existing architecture docs and decision records, spelled as they are on
    disk (listing the directory, never exists(), which ignores case on Windows
    and macOS). A decision folder ends with '/'."""
    root = Path(root)
    found = {"architecture": [], "decisions": []}
    for sub in DOC_DIRS:
        base = root / sub if sub else root
        if not base.is_dir():
            continue
        for p in sorted(base.iterdir(), key=lambda x: x.name):
            name = p.name.lower()
            r = fsutil.rel(p, root)
            if p.is_file() and name.endswith(".md"):
                if name.startswith("architecture"):
                    found["architecture"].append(r)
                elif name.startswith(("decisions", "adr")):
                    found["decisions"].append(r)
            elif p.is_dir() and sub and name in DECISION_DIRS:
                found["decisions"].append(r + "/")
    return found


def scan(root) -> dict:
    root = Path(root)
    manifests, lockfiles, ci, containers, iac = [], [], [], [], []
    for p in fsutil.walk_files(root, skip_dirs=SKIP):
        r = fsutil.rel(p, root)
        if r.count("/") >= MAX_DEPTH:
            continue
        name = p.name
        kind = MANIFESTS.get(name) or next((k for s, k in SUFFIX_MANIFESTS.items() if name.endswith(s)), None)
        if kind:
            manifests.append({"path": r, "kind": kind, "hints": _hints_for(_deps(p, kind))})
        if name in LOCKFILES:
            lockfiles.append(r)
        if r.startswith(".github/workflows/") or name in (".gitlab-ci.yml", "Jenkinsfile", "azure-pipelines.yml") \
                or r.startswith(".circleci/"):
            ci.append(r)
        if name in ("Dockerfile", "docker-compose.yml", "docker-compose.yaml", "compose.yaml", "compose.yml") \
                or name.endswith(".Dockerfile"):
            containers.append(r)
        if name.endswith(".tf") or name == "Chart.yaml" or name in ("serverless.yml", "cdk.json", "Pulumi.yaml"):
            iac.append(r)
        if name in ("artisan",) and r == "artisan":
            manifests.append({"path": r, "kind": "php", "hints": ["laravel"]})
        if re.match(r"(openapi|swagger)\.(ya?ml|json)$", name):
            manifests.append({"path": r, "kind": "openapi", "hints": ["rest-api"]})
    instruction = [f for f in INSTRUCTION_FILES if (root / f).exists() or (root / f).is_symlink()]
    instruction += [d + "/" for d in INSTRUCTION_DIRS if (root / d).is_dir()]
    top = sorted(p.name + "/" for p in root.iterdir() if p.is_dir() and p.name not in SKIP) if root.is_dir() else []
    return {
        "manifests": manifests,
        "lockfiles": lockfiles,
        "ci": ci,
        "containers": containers,
        "iac": sorted(set(iac)),
        "instruction_files": instruction,
        "docs": doc_files(root),
        "top_level_dirs": top,
        "note": "Facts only. The init-project skill interprets them and confirms with the user.",
    }


def detected_platforms(root) -> list:
    root = Path(root)
    found = ["claude-code"]
    if (root / ".codex").is_dir():
        found.append("codex")
    if any((root / p).exists() for p in (".github/copilot-instructions.md", ".github/instructions",
                                          ".github/agents", ".github/prompts")):
        found.append("copilot")
    if (root / ".cursor").is_dir() or (root / ".cursorrules").exists():
        found.append("cursor")
    return found
