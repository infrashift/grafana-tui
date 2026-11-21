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

## Acceptance Criteria
- [ ] An org admin finds the setting that stops push and pull request events, and changes it.
- [ ] A push to `main` starts `ci.yml`, and a pull request starts `ci.yml`.
- [ ] A tag push starts `release.yml`.
- [ ] `RELEASE_PROCESS.md` drops the steps for a run by hand, or keeps them as a fallback.

## Related
[ADR-0003](/decisions/adr-0003-semscope-and-clippy-enforce-the-power-of-10-rules--01a1fd4e487d.md)
