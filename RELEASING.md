# Releasing Pyreal

Pyreal is developed in a private repository. Each release is committed to the public repository,
[jacobjohnston/pyreal](https://github.com/jacobjohnston/pyreal), as one snapshot commit, and its
version tag builds and publishes the release there. Actions are turned off in the private
repository. Nothing builds on an ordinary push or pull request, and either workflow can be run by
hand from the public repository's Actions tab.

## Versions

Pyreal uses [Semantic Versioning](https://semver.org/), starting at 0.1.0, with no tie to Dereth's
version numbers.

- A minor version (0.2.0) brings new features, and may change settings or files.
- A patch version (0.1.1) only fixes things.
- A pre-release carries a suffix (`0.2.0-rc.1`) and is published as a GitHub pre-release.

The version lives in one place, `version` under `[workspace.package]` in the root `Cargo.toml`.
Every crate inherits it, and it shows in `pyreal --version`, the app bundle and the release files'
names. The tag is the version with a `v` in front: `v0.1.0`.

## Cutting a release

1. Merge everything the release should hold into `main`.
2. Write the release's section of [CHANGELOG.md](CHANGELOG.md): a `## [0.2.0] - <date>` heading
   under `## [Unreleased]`, with one line per change, grouped by area. Add the version's link at
   the bottom.
3. Set the new version in `Cargo.toml` and run `cargo check` so `Cargo.lock` follows.
4. Commit both on `main` as `Pyreal 0.2.0`.
5. Run `cargo xtask ci prepush` on the Mac, if the pre-push hook will not (see
   [CONTRIBUTING.md](CONTRIBUTING.md)). Nothing on CI checks the Mac.
6. Push `main` to the private repository and tag it there, as the record of what was released.
   Nothing builds in the private repository.

   ```bash
   git push origin main && git tag -a v0.2.0 -m "Pyreal 0.2.0" && git push origin v0.2.0
   ```

7. Commit the release to the public repository: one commit holding `main`'s tree, on top of the
   public `main`, titled `Features and fixes for v0.2.0` with the version's CHANGELOG section as
   its body. Make it in a scratch bare repository so no private history goes with it, then tag it
   and push both.

   ```bash
   git init --bare /tmp/pyreal-public && git push /tmp/pyreal-public main:refs/private/tip
   ```

   ```bash
   cd /tmp/pyreal-public && git fetch https://github.com/jacobjohnston/pyreal.git main:refs/heads/main
   ```

   ```bash
   c=$(git commit-tree 'refs/private/tip^{tree}' -p main -F message.txt) && git update-ref refs/heads/main $c && git update-ref -d refs/private/tip && git tag -a v0.2.0 -m "Pyreal 0.2.0" main
   ```

   ```bash
   git push https://github.com/jacobjohnston/pyreal.git main v0.2.0
   ```

The Release workflow then:

- checks that the tag matches the version in `Cargo.toml`;
- runs the checks from `ci.yml` (the table below);
- builds the base art pack from the commit `base-pack.toml` pins, fetched from the
  `dereth-enhanced-environment` repository with its LFS files;
- packages Pyreal on each system with `cargo xtask package pyreal` (the Mac disk image, the
  Windows zip, the Linux tarball and the plugin tool for each), signing and notarizing the Mac
  disk image on the way, and starts each packaged program once (`--version`);
- publishes a GitHub Release named `Pyreal 0.2.0` with every file, `SHA256SUMS`, `MANIFEST.txt`,
  `release.json`, and the version's CHANGELOG.md section as its notes.

Each client checks for a newer release when it opens by reading `release.json` from
`releases/latest/download/` in the repository its build names (`PYREAL_BUILD_SOURCE_URL`). It reads
the `product`, `version`, `tag` and `prerelease` fields, so those stay in the index. GitHub's latest
release is never a pre-release, and a stable client is never told of one. A client built without
the source URL never checks. To see the notice from a checkout, build with both
`PYREAL_BUILD_SOURCE_URL=https://github.com/jacobjohnston/pyreal` and an older
`PYREAL_CHECK_AS_VERSION`, such as `0.1.1`.

## What runs where

A release checks each thing once, on the cheapest system that can show it fails. The public
repository's Actions minutes are free, but the Mac's runners are the scarcest, so the Mac's own
checks run before the push.

| Check | Where | Why there |
|---|---|---|
| The rules and the docs | CI, Linux, once | They read the same source on every system |
| Every no-data test | CI, Linux | What fails off the Mac shows here, at the lowest price |
| Clippy | CI, Linux and Windows | It reads the code each system compiles alone, quickly |
| The tests of the crates with Windows code (`cargo xtask ci smoke`) | CI, Windows | Their Windows paths run nowhere else |
| The build, the tests and clippy on the Mac | The Mac, before the push (`cargo xtask ci prepush`) | The Mac is where Pyreal is made and played |
| The shipped build, its packaging and that it starts | CI, each system's package job | Only the system itself can build, link, sign and start it |

## When something fails

- A check or package fails: fix it on `main`, then move the tag and push it again.

  ```bash
  git tag -d v0.2.0 && git push origin :refs/tags/v0.2.0
  ```

- The publish step fails after the packages built: run the Release workflow by hand on the tag
  (Actions, Release, Run workflow, choose the tag). A rerun on a tag that already has a Release
  replaces its files and keeps its notes.

## Manual runs and caches

- Running CI by hand checks a branch, on Linux and Windows. Running Release by hand on a branch
  also packages it and keeps the packages as the run's artifacts for 7 days. Only a run on a
  version tag publishes a Release.
- Build caches are saved only by runs on `main`, and a tag's run reads `main`'s. Running CI by hand
  on `main` before tagging warms the checks' caches. Running Release by hand on `main` now and then
  also keeps the package builds and the base pack cached, which saves fetching the pack's LFS
  files (about 500 MB) again, but it spends Mac minutes.
- On Windows, `mach-dxcompiler` downloads DXC from GitHub while it builds. The workflows give its
  API call the run's token, so shared runners don't run out of unauthenticated requests.

## What the packages are

- The Mac release is `Pyreal-<version>-macos-aarch64.dmg`: `Pyreal.app` beside a link to
  Applications, in a Finder window with Pyreal's parchment picture behind large icons. The
  packager writes the window's settings itself, so a release runner makes the same window; the
  picture is `pyreal/client/assets/dmg-background.svg`, rendered by `render-dmg-background.py`
  beside it. The app is signed with the Developer ID under the hardened runtime, with Apple's
  secure timestamp. The disk image is signed, sent to Apple's notary service, and stapled once
  Apple accepts it, so it opens without a warning even offline. A rejection fails the run and
  prints Apple's log. The app is not sandboxed: notarizing does not need it, and Pyreal reads the
  player's game folder and art packs wherever they are.
- `python3 build.py` makes the same disk image locally, signed with `build.ini`'s identity and
  not notarized, and copies `Pyreal.app` out of it.
- The Windows zip and the Linux tarball are not signed.

## Mac signing secrets

These live in the public repository's `release` environment, which only `v*` tags may use, so no
other run can read them. A version tag fails without them. A run by hand on a branch packages an
ad hoc signed app that is not notarized.

| Secret | What it holds |
|---|---|
| `MACOS_CERTIFICATE_P12` | The Developer ID Application certificate and its key, exported as a `.p12`, in base64 |
| `MACOS_CERTIFICATE_PASSWORD` | The `.p12`'s password |
| `MACOS_SIGNING_IDENTITY` | The certificate's name, `Developer ID Application: <name> (<team>)` |
| `MACOS_TEAM_ID` | The Apple developer team id |
| `MACOS_KEYCHAIN_PASSWORD` | Any password, for the keychain the run makes and deletes |
| `APPSTORE_API_KEY_P8` | An App Store Connect Team API key with the Developer role, the `.p8` file's text |
| `APPSTORE_API_KEY_ID` | That key's id |
| `APPSTORE_API_ISSUER_ID` | The Issuer ID shown above the team keys |

The packager reads them as `APPLE_SIGNING_IDENTITY`, `APPLE_API_KEY_PATH`, `APPLE_API_KEY` and
`APPLE_API_ISSUER`; set those to notarize a package made by hand on a Mac.
- The Linux package is built on Ubuntu 22.04 and runs on its glibc or newer.
- No package holds game data. The base art pack is the one data file a package carries.
