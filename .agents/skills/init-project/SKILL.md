---
name: init-project
description: Set up hub for this repository by analyzing the codebase and writing the project-specific layer (the AGENTS.md project section, the architecture doc, .agents/project.json, packs and platforms), or adopting the instructions and docs a mature repository already has. Use right after `hub install`, or when AGENTS.md still says it is not set up.
disable-model-invocation: true
---

# Initialize the project layer

Goal: an accurate, short project section in `AGENTS.md` and an architecture doc, both grounded in what the repository actually contains, plus a `project.json` that enables the right packs and platforms. You **propose**; the user **approves**; then you write.

A repository that already has substantial project instructions or docs is **adopted**, not rewritten: follow [Adopting an existing setup](#adopting-an-existing-setup) instead of sections 2 and 3.

## 1. Gather facts (read-only)

1. Run `python .agents/tools/hub detect` (or read `.agents/state/detect.json`). It lists manifests, dependency hints, lockfiles, CI, containers, infrastructure and existing instruction files. These are facts, not conclusions.
2. Read the existing instruction files it lists (`AGENTS.md`, `CLAUDE.md`, `.cursor/rules`, `.github/copilot-instructions.md`, `.github/instructions/`, `.claude/rules/` and so on). **These hold the team's current conventions. Harvest them; don't contradict them.**
3. Read the README, the main manifests, CI workflows (the real build and test commands live there) and the top-level layout. Open 2-3 representative source files per major area to learn the conventions.
4. Classify the project with [references/project-types.md](references/project-types.md). Note what that type needs documented.

For a new or nearly empty repository, ask the user for: purpose, intended stack, the commands they expect, and constraints. Then propose a minimal structure instead of inferring one.

## Adopting an existing setup

Use this when `AGENTS.md` already has real project text (no `<!-- hub:init-required -->` marker), or `docs` in `detect.json` lists an architecture doc or decision record. The existing setup is the source of truth; hub adapts to it.

1. **Map, don't replace.** Keep the project's `AGENTS.md` text, docs and decision format. Never write a second architecture doc or decision record next to the ones that exist.
2. **Point hub at them.** Propose `docs.architecture` and `docs.decisions` in `.agents/project.json`, using the exact spelling from `detect.json` (a decision folder ends in `/`). Propose `commands` from the commands the project already documents or runs in CI.
3. **List the conflicts** between the hub standards and the project's rules (branching, commit style, task tracking, where decisions go). Propose a short section in the project text that states which project rule wins. Project text already overrides the hub block, so this just makes it explicit.
4. **Propose disabling** hub skills or agents that duplicate the project's own workflows, via `disabled` in `project.json`.
5. **Fill gaps only:** a missing Commands list, a missing boundary, an `AGENTS.md` over ~150 lines. Propose each one separately.

After approval, write only what was approved, then run `python .agents/tools/hub sync` (it renders the new paths and commands into the hub block) and `python .agents/tools/hub validate`. Report as in section 4.

## 2. Propose (show the user, wait for approval)

Present all of these together:
- **AGENTS.md project section**, filling the template headings: Overview, Stack, Commands (only commands you found evidence for; mark the rest `unknown`), Layout, Conventions, Boundaries. Target ≤ 100 lines. Mark anything inferred rather than seen as *(assumption)*.
- **Platforms** to enable, from the instruction files and tools present.
- **Packs** to enable. Match `hub.available_packs[].detect` in `detect.json` against the manifest hints. If `hub.available_packs` is empty, ask which packs the user wants.
- **Architecture doc** (`docs/architecture.md` unless the project already has one) outline from the [template](../../templates/architecture.md): components, data, key flows, deployment, all from evidence.
- **Consolidation advice** if instructions are split or oversized. For example: CLAUDE.md content that should move into AGENTS.md; an AGENTS.md over ~150 lines that should move area-specific parts into nested `AGENTS.md` files or `.agents/rules/`; contradictory rules across files.
- **Gaps:** what you couldn't determine and need the user to answer.

## 3. Write (after approval)

1. Replace everything below the hub core block in `AGENTS.md` with the approved section, and remove the `<!-- hub:init-required -->` marker. Never edit inside the core block.
2. Write the architecture doc. If there are significant past decisions worth recording, propose ADRs in the decision record rather than writing many unasked.
3. Update `.agents/project.json`: `name`, `platforms`, `packs`, `stack` (a short structured summary), `commands` (`build`, `test`, `lint`, `format`, `run` as found) and `docs` if the paths differ from the defaults.
4. Packs: if the user approved packs, run `python .agents/tools/hub install . --packs <names> --source <hub checkout>`.
   - The checkout is `hub.source` in `detect.json`.
   - If that is `null` (the project was made from the template, which no longer contains packs), ask for the path to a hub clone, or tell the user the command to run.
   - If install says the project has an older hub version than the checkout, run `python .agents/tools/hub update --source <checkout>` first, then add the packs.
5. Move existing instructions only with explicit approval. When moving, keep wording; never silently drop a rule.
6. Run `python .agents/tools/hub sync`, then `python .agents/tools/hub validate`, and fix any errors.

## 4. Report

List the files written, the assumptions to confirm, the unanswered gaps, any validate warnings, and the suggested next steps. For example: enable the `claude_hooks` option, copy `.agents/templates/ci/hub-validate.yml` into CI, record ADRs.
