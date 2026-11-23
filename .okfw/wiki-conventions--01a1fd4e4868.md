---
x-okfw-id: "01a1fd4e-4868-73f5-b4e3-0a9ee9d25827"
title: "Wiki conventions"
type: "Guide"
tags:
  - "conventions"
  - "meta"
x-okfw-created: "2026-11-21T02:52:08-06:00"
x-okfw-timestamp: "2026-11-21T02:52:08-06:00"
description: "Layout, writing style and authoring rules for the architecture notes, ADRs, issues and daily notes in this wiki."
---

# Wiki conventions

This guide defines how the grafana-tui OKF wiki (`.okfw/`) stores project knowledge.

- Create concepts with `okfw create`. Do not write frontmatter by hand.
- Search before you write. Link to related concepts.
- Keep one idea in each concept. Give each concept a one-sentence `description`.
- Do not start a `--body` with `# Title`. okfw writes the H1 from the title.

## Layout
| Path | Type | Contents |
|---|---|---|
| `architecture/` | `Architecture` | The system overview, the module boundaries and the data flow. |
| `decisions/` | `ADR` | Architecture decision records. |
| `issues/` | `Issue` (kind `issue`) | Tracked work: one issue for each refactor phase, and bugs and design debt. |
| `daily/` | daily note | `okfw daily`: what changed, what you learned, what is next. |

## ADRs (`decisions/`)
- Title: `ADR-NNNN: <decision>`. Number the ADRs in sequence. Do not use a number again.
- Command: `okfw create "ADR-NNNN: <title>" --path decisions --type ADR --tags adr,<topic> -d "<one sentence>" --body "<markdown>"`
- Body: `Status` (Proposed, Accepted, or Superseded by ADR-XXXX), `Date`, `Source`, then the sections `Context`, `Decision`, `Consequences` and `Alternatives considered`.
- To replace an ADR, write a new ADR with a link to the old ADR. Change the Status line of the old ADR. Run `okfw set-lifecycle <id> deprecated`.

## Issues (`issues/`)
- Title: `ISSUE-NNN: <summary>`. Number the issues in sequence.
- Command: `okfw create "ISSUE-NNN: <title>" --kind issue --path issues --type Issue --tags issue,open,phase-<n>,<area> -d "<summary>" --body "<markdown>"`
- Body: a `Status/Phase` line, then the sections `Description`, `Acceptance Criteria` (checkboxes), `Notes` and `Related`.
- Tags carry the state: `open`, then `in-progress`, then `closed`. Use `okfw remove-tags` and `okfw add-tags`. Keep the Status line the same as the tags.
- Add a `Resolution` section when you close an issue.
- Find open work with `okfw search --path issues --tag open`.

## Writing style
All documentation follows ASD-STE100 Simplified Technical English. This includes the wiki, the docs site, `README.md` and the code comments.

- Use the active voice and the simple present tense: "The importer rejects the panel."
- Start each procedural step with a verb in the imperative: Run, Read, Add, Set.
- Keep procedural steps to 20 words or fewer. Keep descriptive sentences to 25 words or fewer.
- Use a verb, not a noun form of the verb.
- Use one term for one concept: *dashboard*, *panel*, *row*, *tab*, *layout tree*, *variable*, *query*, *series*, *annotation*, *refresh*.
- Do not use `should`, `will`, `would`, `please`, `simply`, `just`, `very`, `really` or `easily`.
- Run `make docs-style`. lexiscope checks the text with the `ste-100` ruleset and the terms in `vocabulary.cue`.

## Housekeeping
- Use `okfw move` and `okfw delete`. Do not use `mv` or `rm`. The okfw commands repair the links and the `index.md` listings.
- `status` is the OKF lifecycle field (`draft`, `stable` or `deprecated`). It is not the state of a task.
- Do not edit `index.md` or `log.md`. okfw makes them.
- Run `make wiki` before you commit changes to the wiki.
