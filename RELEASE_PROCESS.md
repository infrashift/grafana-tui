# Release Process

A maintainer releases grafana-tui with a push of a version tag. No release bot
exists: no workflow bumps versions, opens pull requests or creates tags. Only
`release.yml` can write to the repository. It only creates the GitHub release
for the tag that you pushed.

grafana-tui is available only from GitHub Releases. We do not publish it to
crates.io or to a third-party package manager. Users install the release
archives with `install.sh`. They can also copy the binary by hand into
`~/.local/bin` or into another directory on their `PATH`.

## Workflows

| Workflow | Runs on | Permissions | Does |
|---|---|---|---|
| `ci.yml` | Every push to `main`, every pull request | read | The quality gate below |
| `docs-release.yml` | Pushes to `main` that touch `docs/` | Pages | Builds the user guide and deploys it to GitHub Pages |
| `release.yml` | Pushing a `vX.Y.Z` tag | read; `publish` job only: write | Builds the archives and creates the GitHub release |

### The quality gate

`ci.yml` runs these checks on Linux and macOS:

- `make fmt`. The job fails if `make fmt` changes a file.
- `make lint`: clippy on each target, with each warning as an error.
- `make test`.
- `make test-install`: the `install.sh` tests.
- `make cover-check`: the job fails when the line coverage is less than 95%.
- `make vuln`: `cargo audit` against the RustSec advisory database.

`make check` also runs the `p10`, `wiki` and `docs-style` gates. CI cannot run
them, because semscope, lexiscope and okfw have no public releases yet (wiki
ADR-0003). Run `make check` on the local computer before each push.

It also runs:

- **`windows-build`:** `cargo build` and clippy on Windows.
- **`release-shape`:** `make tidy`, then `make package`. `make tidy` fails if
  `Cargo.lock` is out of date. The job runs the `grafana-tui` binary from the
  root of the archive, where `install.sh` expects it.
- **`docs`:** `make docs-build`. A broken page fails the review, not the
  deploy.

A pull request that fails the gate cannot merge.

## Cutting a Release

1. Start from an up-to-date `main` whose CI run passed.
2. Choose the version, as [SemVer](https://semver.org/) tells you.

   grafana-tui is pre-1.0. An incompatible change bumps the minor version.
   Other changes bump the patch version.
3. Set `version` in `Cargo.toml`. Refresh `Cargo.lock`:

   ```bash
   cargo update --workspace
   ```

4. Make the changelog again. This step needs
   [git-cliff](https://git-cliff.org/) (`cargo install --locked git-cliff`):

   ```bash
   make changelog
   ```

   It adds a section to the top of the changelog. git-cliff makes the
   section from the conventional commits since the last tag, through
   `cliff.toml`. The earlier entries do not change. The first release of the
   fork has no tag to start from. For that release, give the commit that
   completed the 0.1.12 changelog: `make changelog SINCE=ee4c529`. Examine
   the new section, and edit it by hand if necessary.
5. Commit the change, add the tag, and push both:

   ```bash
   git commit -am "chore(release): prepare for v$(make -s version)"
   git push origin main
   git tag "v$(make -s version)"
   git push origin "v$(make -s version)"
   ```

The push of the tag starts `release.yml`. After the repository moved to the
infrashift GitHub account, a push starts no workflow run (wiki ISSUE-013).
Until a fix comes, start each run by hand:

1. Run `gh workflow run ci.yml --ref main` before you add the tag.
2. Make sure that the CI run passes.
3. Push the tag.
4. Run `gh workflow run release.yml --ref "v$(make -s version)"`.

A run by hand on a branch stops in `verify`, because it has no tag.

`release.yml` has these jobs:

1. **`verify`:** fails unless the tag matches the `Cargo.toml` version.
2. **`package`:** `make package` builds each archive natively, on
   read-only runners:
   - `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`, on Ubuntu
     22.04, so the binaries need only glibc 2.35
   - `x86_64-apple-darwin` and `aarch64-apple-darwin`
   - `x86_64-pc-windows-msvc`, as a `.zip`
3. **`publish`:** the only job with write access. It runs `make checksums` to
   write `grafana-tui-checksums.txt`, then `gh release create` with every
   archive and the manifest, and GitHub-generated notes.

Then verify that the release lists five archives and
`grafana-tui-checksums.txt`. Also verify that `install.sh` installs it:

```bash
GRAFANA_TUI_VERSION="v$(make -s version)" bash install.sh
```

If a release job fails, fix the cause on `main`. Then delete the tag, and
push it again:

```bash
git push --delete origin vX.Y.Z && git tag -d vX.Y.Z
```

## Building Release Archives Locally

```bash
make package                                  # host target, into ./dist
make package TARGET=aarch64-unknown-linux-gnu # needs that target's toolchain
make checksums                                # dist/grafana-tui-checksums.txt
```

Each tarball holds `grafana-tui`, `README.md`, `LICENSE` and `NOTICE` at its
root.

## Commit Message Format

git-cliff makes the changelog from [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

| Type | Use for | Changelog |
|------|---------|-----------|
| `feat` | New feature | Features |
| `fix` | Bug fix | Bug Fixes |
| `docs` | Documentation only | Documentation |
| `perf` | Performance improvement | Performance |
| `refactor` | Code refactoring | Refactor |
| `style` | Code style/formatting | Styling |
| `test` | Adding tests | Testing |
| `chore`, `ci` | Maintenance | Miscellaneous Tasks |

The changelog does not show the `chore(release): prepare for …` and
`chore(deps…)` commits.

To mark an incompatible change, put `!` after the type, or add a
`BREAKING CHANGE:` footer:

```bash
git commit -m "refactor(app)!: change AppState constructor signature

BREAKING CHANGE: AppState::new() now requires a Theme parameter"
```

Recommended scopes: `app`, `ui`, `prom`, `grafana`, `config`, `zoom`, `theme`.

## Repository Settings

- **Actions → General → Workflow permissions:** read repository contents
  (the default). Leave "Allow GitHub Actions to create and approve pull
  requests" off; no workflow needs it.
- **Pages → Source:** GitHub Actions.
- **Secrets:** none. Every workflow uses its own short-lived `GITHUB_TOKEN`.
