# Releasing pheobe

Version moves with the work (fleet doctrine, `squadron/bin/bump-version`).
Nothing here is run by hand after the first release.

```text
merge to main ──► ci.yml (fmt, clippy, test, package)
              └─► release.yml: gate (fmt, clippy, test) ──► scripts/bump-version.sh
                     feat:            → minor      ┐
                     fix: / perf: …   → patch      ├─ commit "chore(release): vX.Y.Z [skip ci]"
                     feat!: / BREAKING → major     ┘  push commit, THEN tag, GitHub release
                     docs/chore/test/ci → no-op (no tag)
tag vX.Y.Z ───► publish.yml: tag == Cargo.toml version? not already on crates.io?
                     cargo publish --dry-run ──► cargo publish
```

A tag the bot pushes with `GITHUB_TOKEN` raises no `push` event (GitHub's
recursion guard), so `release.yml` ends by `gh workflow run publish.yml
--ref vX.Y.Z` — `workflow_dispatch` is the one event that guard allows. A
hand-pushed tag still triggers `publish.yml` directly. Either way the
publish is idempotent: an already-published version is skipped.

## First release (by hand, once)

`bump-version.sh` refuses to invent a version: with no prior `v*` tag it
exits 0 ("tag v0.1.0 by hand"). So the first release is (history: v0.1.0
was tagged 2026-09-16; the first crates.io version is 0.2.0, minted by the
bot from the first `feat:` merge after it):

```sh
git tag -a v0.1.0 -m "pheobe v0.1.0"
git push origin v0.1.0            # → publish.yml
```

Every later version is minted by `release.yml` from conventional-commit
subjects on `main`.

## Credentials

`publish.yml` tries crates.io **Trusted Publishing** first (OIDC; configure
on crates.io → crate settings → Trusted Publishing → GitHub, repository
`nixpt/pheobe`, workflow `publish.yml`) and falls back to a
`CARGO_REGISTRY_TOKEN` repository secret. Trusted Publishing can only be
configured on a crate that already exists, so the very first publish uses
the token (secret or a local `cargo publish`); after that the secret can go.
**State (2026-09-16):** Trusted Publishing is configured and verified
(`verify_auth` run); the repo holds no `CARGO_REGISTRY_TOKEN` secret.

`release.yml` pushes as `pheobe-release` with the workflow's own
`GITHUB_TOKEN`; the `fleet-default-main-protection` ruleset (deletion +
non-fast-forward only) permits that fast-forward push. If the ruleset ever
gains a `pull_request` rule, the bump push is refused *before* the tag is
pushed (zorro#128 ordering) and the run fails loudly — no orphan tag.

## Kit provenance

`scripts/bump-version.sh` and `release.yml` are installed copies of
`squadron/bin/bump-version --install`; edit there and re-install, not here.
`publish.yml` follows `api-drift`'s (the fleet's first crates.io crate).
