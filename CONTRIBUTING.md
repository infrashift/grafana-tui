# Contributing to grafana-tui

Thank you for the help with grafana-tui.

We accept bug reports, feature requests, documentation and code. This guide
tells you how to send each type of change. Read the applicable section before
you start.

## Table of Contents

- [Ask a Question](#ask-a-question)
- [Send a Change](#send-a-change)
  - [Report a Bug](#report-a-bug)
  - [Request a Feature](#request-a-feature)
  - [Write Code for the First Time](#write-code-for-the-first-time)
- [Workflow](#workflow)
- [Pull Requests](#pull-requests)

## Ask a Question

Before you ask a question, read the [documentation](README.md).

Then search the [issues](https://github.com/infrashift/grafana-tui/issues).
If an issue answers part of the question, add the question to that issue.

## Send a Change

### Report a Bug

A good bug report helps the maintainers find the cause. Put these items in the
report:

- A clear title that names the problem.
- The exact steps that cause the problem.
- Examples for the steps, for example links to files or snippets that you can copy.
- The behavior that you saw after the steps.
- The behavior that you expected, and the reason.
- Screenshots or animated GIFs that show the problem.

### Request a Feature

Use an issue to request a new feature or a small change to a feature. Put these
items in the request:

- A clear title that names the feature.
- A description of the feature, step by step.
- Examples for the steps.
- The current behavior, the behavior that you expect, and the reason.

### Write Code for the First Time

These issue lists are a good start:

- [Good First Issues](https://github.com/infrashift/grafana-tui/issues?q=is%3Aopen+is%3Aissue+label%3A%22good+first+issue%22): issues that need a few lines of code and one or two tests.
- [Help Wanted Issues](https://github.com/infrashift/grafana-tui/issues?q=is%3Aopen+is%3Aissue+label%3A%22help+wanted%22): issues that need more work than the first issues.

## Workflow

[DEVELOPMENT.md](DEVELOPMENT.md) gives the full workflow. This section gives
the rules that each change must obey.

### Tools

- Install the Rust tools with `make tools`.
- Put `semscope`, `lexiscope` and `okfw` on the `PATH`.

The three tools have no public releases yet. CI runs only
`make lint test test-install cover-check` (wiki ADR-0003). So the
`p10`, `wiki` and `docs-style` gates run only on the local computer. Run
`make check` before each push.

### Build Files

The `Makefile` (GNU Make) and the `ZMakefile` (zmake) have the same targets.
Keep the two files in step (wiki ADR-0005). If you change one file, make the
same change in the other file.

### Gates

`make check` and `zmake check` run these gates:

- `fmt`: rustfmt.
- `lint`: clippy with `-D warnings`.
- `test`: the unit tests, the CLI tests in `tests/` and the snapshot tests.
- `test-install`: the `install.sh` tests.
- `cover-check`: the line coverage must be 95% or more.
- `p10`: semscope with `.semscope.cue`, then `scripts/p10-assert-density.py`.
- `wiki`: `okfw validate --all`.
- `docs-style`: lexiscope with `vocabulary.cue`.

### Power of 10 Rules

The production code follows the NASA Power of 10 rules. The wiki ADR-0002
adapts them to Rust. In short:

- A function has 60 lines or fewer.
- The code has no recursion.
- Each loop has a bound.
- A named limit caps each input and each growable collection.
- The functions of more than 5 lines have an average of 2 or more assertions.
- The code has no `unwrap`, no `expect`, and no `as` outside `src/num.rs`.
- The code has no `dyn` and no function pointers.

### Working Loop

1. Write the code for one small slice of the change.
2. Run `make fmt lint p10` to lint the change.
3. Run `make test test-install cover-check` to verify the change.
4. Repeat steps 1 to 3 until all of the gates pass.
5. Update the wiki, as the wiki guide `.okfw/development-workflow--*.md` tells you.
6. Run `make check`.
7. Commit the change.

### Snapshot Tests

`src/snapshot_tests.rs` compares the TUI and SVG output of each example
dashboard with the files in `tests/snapshots/`. If you change the output on
purpose, record new snapshots:

1. Run `UPDATE_SNAPSHOTS=1 cargo test snapshot_tests`.
2. Examine the diff of `tests/snapshots/`.
3. Commit the new snapshots with the change.

### Writing Style

Write the comments and the documentation in ASD-STE100 Simplified Technical
English (wiki ADR-0004). `make docs-style` checks them.

## Pull Requests

1. Fork the repository.
2. Make a branch from `main`.
3. Add tests for each new behavior.
4. Update the documentation for each API change.
5. Run `make check`. All of the gates must pass.
6. Write each commit message in the [Conventional Commits](https://www.conventionalcommits.org/) format.
7. Open the pull request.

The changelog and the releases come from the commit messages. Examples:

- `feat: add new feature`
- `fix: fix bug`
- `docs: update documentation`
- `chore: maintenance`

## License

When you send a change, you agree that the change has the Apache 2.0 License of this project.
