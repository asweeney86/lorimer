# Contributing to Lorimer

Thanks for your interest. Bug reports, fixes, and focused features are all welcome. For anything larger than a small fix, please open an issue first so we can agree on the approach before you spend time on it.

## Getting set up

You need Rust 1.88 or newer on macOS, Linux, or Windows.

```bash
git clone https://github.com/asweeney86/lorimer.git
cd lorimer
cargo run --release
```

`LORIMER_AUTOSCAN=<path> cargo run --release` opens straight into a scan, which is handy while working on the interface.

## Before you open a pull request

Run the same three checks that CI runs. All of them must pass.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Then check your change against these expectations:

- **Tests.** Behavior changes come with a test. Bug fixes come with a regression test that fails without the fix.
- **Scope.** One change per pull request. Leave unrelated refactors and reformatting out.
- **Scanner performance.** If you touch the scanner, compare `scan-bench` on a release build before and after and include the numbers in the pull request.
- **Interface changes.** Launch the app, look at what you changed, and attach a screenshot.
- **Dependencies.** The dependency list is short on purpose. Say why a new one is needed.

## How the code is organized

| Path | What lives there |
| --- | --- |
| `crates/core` | Scanner, scan snapshot model, metadata, platform actions. No interface code. |
| `crates/gui-iced` | The desktop app, built with [iced](https://iced.rs). |
| `crates/cli` | Command-line frontend and the `scan-bench` benchmark. |
| `assets` | Logo sources and generated icons. |
| `packaging/homebrew` | Template for the Homebrew cask, filled in by CI on each release. |
| `docs/architecture.md` | Design rules and the reasoning behind them. |

The rules that keep these pieces separate (for example, the core crate never depends on a GUI type) are written down in [docs/architecture.md](docs/architecture.md).

## Screenshots

The screenshots in the README are taken from a generated folder tree, so they contain nobody's real files. To reproduce them:

```bash
python3 scripts/demo-tree.py /path/to/empty/dir
LORIMER_AUTOSCAN=/path/to/empty/dir/Users/alex cargo run --release
```

The files in the demo tree are sparse, so the tree reports hundreds of gigabytes while using almost no disk space.

## Releasing

1. Bump `version` in the root `Cargo.toml`, move the `Unreleased` entries in `CHANGELOG.md` under the new version, and merge.
2. Push a tag such as `v0.1.1`. The Release workflow builds the downloads and attaches them to a draft release.
3. Review the draft on GitHub and publish it.
4. Publishing runs the Homebrew workflow. It renders `packaging/homebrew/lorimer.rb.in` with the new version and checksum, pushes it to [asweeney86/homebrew-tap](https://github.com/asweeney86/homebrew-tap), and then installs from the tap on a clean macOS runner to check it.

The Homebrew workflow needs the `HOMEBREW_TAP_TOKEN` repository secret: a fine-grained token with read and write access to the contents of the tap repository only. If the workflow fails, or the secret was missing, re-run it from the Actions tab with "Run workflow" and the release tag. Do not edit the cask in the tap by hand; the next release overwrites it.

## Reporting bugs

Please include your operating system and version, how you installed or built Lorimer, and what you scanned (a rough description is enough: "my home folder, about 2 million files"). If a scan reports a size you believe is wrong, the output of `lorimer-cli scan <path>` for the smallest folder that shows the problem is the most useful thing you can attach.

## License

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
