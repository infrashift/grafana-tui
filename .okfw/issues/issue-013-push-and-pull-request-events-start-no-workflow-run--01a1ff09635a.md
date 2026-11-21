---
id: "01a1ff09-635a-7504-b1e3-b5d4380c47ae"
title: "ISSUE-013: Push and pull request events start no workflow run"
type: "Issue"
kind: "issue"
tags:
  - "issue"
  - "open"
  - "ci"
  - "release"
created: "2026-11-21T10:56:07-06:00"
timestamp: "2026-11-21T10:56:07-06:00"
description: "Since the move to the infrashift account, only a manual workflow_dispatch starts a GitHub Actions run."
---

# ISSUE-013: Push and pull request events start no workflow run

**Status/Phase:** Open · debt

## Description
A push to `main`, a pull request and a tag push start no GitHub Actions run in `infrashift/grafana-tui`. A manual run (`workflow_dispatch`) works. The last runs that a push started were before the repository moved to the infrashift GitHub account. The org settings for Actions are not visible to a token without the `admin:org` scope.

Effects:
- CI does not check a pull request. A maintainer starts it with `gh workflow run ci.yml --ref <branch>`.
- A docs change on `main` does not deploy the docs site.
- A version tag does not start `release.yml`. `release.yml` now accepts `workflow_dispatch` on a tag ref, and `RELEASE_PROCESS.md` gives the steps.

## Investigation
On 2026-11-21 an org admin token examined the settings:
- The org allows Actions for all repositories and all actions. The default workflow token can only read. The free plan has no org rulesets.
- The repository has Actions on, with all actions. It is public, it is not a fork or a mirror, and it has no webhooks. The three workflows are active, and `ci.yml` has `push` and `pull_request` triggers.
- GitHub records the push events and the pull request events. All 8 runs of the repository came from `workflow_dispatch`.
- The other infrashift repositories get runs from push and pull request events, from the same account.

No setting stops the events. The fault is in this repository only. The admin set Actions and the three workflows off and then on again, with the same settings. A push to `main` then tests the result. If no run starts, GitHub Support must examine the repository.

## Acceptance Criteria
- [x] An org admin examines the settings that can stop push and pull request events. No setting stops them.
- [ ] A push to `main` starts `ci.yml`, and a pull request starts `ci.yml`.
- [ ] A tag push starts `release.yml`.
- [ ] `RELEASE_PROCESS.md` drops the steps for a run by hand, or keeps them as a fallback.

## Related
[ADR-0003](/decisions/adr-0003-semscope-and-clippy-enforce-the-power-of-10-rules--01a1fd4e487d.md)
