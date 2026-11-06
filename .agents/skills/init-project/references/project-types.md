# Project types: what to look for and what to document

Use the evidence from `hub detect`. A repository can be several types (a monorepo usually is). Document each part.

| Type | Evidence | Commands to find | Must document | Usual packs |
|---|---|---|---|---|
| Web app (full-stack) | Framework deps (Next.js, Laravel, Rails, Django + frontend), `app/`, `pages/`, `resources/`, templates | dev server, build, test (unit + e2e), migrations | Routing layout, server/client boundary, auth, data access, env vars | language + framework |
| Backend service | HTTP framework deps, `Dockerfile`, health endpoints, queues | run, test, migrate, lint | Service boundaries, config and secrets source, observability, background jobs | language |
| API | OpenAPI/proto/GraphQL schema files, routers/controllers | contract tests, schema generation | Source of truth for the contract, versioning, error format, auth ([api template](../../../templates/api.md)) | language |
| Desktop app | Electron, Tauri, Qt, WPF/WinUI, egui/bevy deps; packaging config | dev run, package/bundle, sign | Process model (main/renderer or core/UI), IPC, packaging, update channel, OS-specific code | language |
| Mobile app | React Native/Expo, Flutter, `android/`, `ios/`, Swift/Kotlin projects | run on simulator, build per platform, test | Platform folders, native modules, release signing (never secrets), store config | language |
| CLI tool | Arg-parsing deps (clap, click/typer, commander, cobra), `bin/` entry | build, run with `--help`, test | Commands and flags, exit codes, config files, distribution | language |
| Library / package | Package metadata with exports, no app entry, `src/` + `tests/` | build, test, lint, docs build, publish (dry run) | Public API surface, semver policy, supported runtimes, release process | language |
| Data engineering | Airflow/dbt/Spark/Dagster deps, `dags/`, `models/`, SQL | run pipeline locally, tests, data checks | Sources and sinks, schedules, idempotency, data contracts, backfills | language |
| AI / ML | torch/tensorflow/sklearn/transformers deps, notebooks, `data/`, `models/`, experiment configs | train, eval, test, notebook execution | Data provenance and size (never commit large data), experiment tracking, eval metrics, reproducibility (seeds, versions), GPU needs | python |
| Infrastructure / DevOps | Terraform, Helm, Pulumi, k8s manifests, Ansible | fmt, validate, plan (never apply without approval) | Environments, state backends, blast radius, approval rules for apply/destroy | none or iac pack |

## Always capture

- Exact install, build, test, lint and run commands, preferably copied from CI.
- Where config and secrets come from (names only).
- Generated or vendored directories agents must not edit.
- Anything destructive that needs approval (migrations, deploys, data scripts).
