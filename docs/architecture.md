# Architecture

Lorimer is a Cargo workspace with three crates. This document records how they fit together and the rules that keep them that way.

## Crates

| Crate | Package | Role |
| --- | --- | --- |
| `crates/core` | `lorimer-core` | Scanner, scan model, metadata, platform actions, volume capacity. |
| `crates/gui-iced` | `lorimer` | The desktop app, built with [iced](https://iced.rs). |
| `crates/cli` | `lorimer-cli` | Command-line frontend and the `scan-bench` benchmark. |

The core crate knows nothing about any interface toolkit. Both frontends depend on it, and it depends on neither. That is what lets the app and the command-line tool report identical numbers, and it leaves room for other frontends.

## The scan

`ScanService::scan` walks a directory tree and returns a `ScanSnapshot`. `ScanService::stream` does the same while sending progress events over a channel.

**Parallelism.** Each directory's children are scanned with `rayon`, so the work spreads across all cores without the scanner managing threads itself.

**Directory enumeration.** On macOS the scanner calls `getattrlistbulk`, which returns the name, type, size, and link count of many entries in one system call and avoids a `stat` per file. Other platforms use `std::fs::read_dir`; on Windows the listing already carries each file's size, so the scanner takes it from there and never opens the file. All of the `unsafe` code for this lives in the core crate and checks every offset before reading from the kernel's buffer.

**What is counted.**

- Sizes are logical file sizes.
- On Unix, a hardlinked file is counted the first time the scanner meets it and contributes nothing after that. Windows does not report link counts in a directory listing, so hardlinks are not deduplicated there.
- Symbolic links are not followed, and neither are Windows junctions and volume mount points. They are counted as skipped.
- On Unix, a scan never crosses onto another filesystem. Mount points inside the scanned folder are counted as skipped, which keeps a scan of `/` away from other disks and from virtual filesystems such as `/proc`.
- On macOS the data volume is visible twice from `/`: directly at `/System/Volumes/Data`, and through firmlinks such as `/Users`. A scan of `/` keeps the firmlinked paths and skips their duplicates on the data volume, so user data is counted once and appears under the paths people recognize.
- A directory that cannot be read adds to the error count and the scan carries on.

**Cancellation.** `ScanService::stream_with_cancel` takes a `ScanCancel` handle. The scanner checks it before each directory and file, so a cancelled scan winds down in well under a second even on a large tree. It then reports `ScanEvent::Cancelled` and returns no snapshot, because a tree with abandoned directories would show wrong totals.

**Keeping the snapshot small.** A disk can hold millions of files, and a chart cannot show them individually. Below a fixed depth, and beyond the largest files in each directory, files are folded into a single "smaller files" node that carries their combined size. Directory totals stay exact. The largest files across the whole scan are tracked separately, so that list is not affected by the folding.

**The snapshot.** A `ScanSnapshot` is immutable. Nodes live in a flat vector and refer to each other by `NodeId`. Frontends hold ids, not paths, so selection and navigation never depend on string matching.

## Other core services

- `MetadataService` reads dates, permissions, and allocated size for one node and returns them in a struct both frontends can render directly.
- `SystemActions` opens a path or reveals it in the file manager. Frontends never start `open`, `xdg-open`, or Explorer themselves.
- `VolumeService` reports the capacity of the volume that holds a path.

## The desktop app

The app follows iced's model: one state struct (`Lorimer`), a `Message` enum, an `update` function that is the only place state changes, and view functions that draw the current state.

| Module | Contents |
| --- | --- |
| `app.rs` | State, messages, `update`, scan streaming, the toolbar and status bar. |
| `chart.rs` | Sunburst layout, hit testing, and drawing. |
| `sidebar.rs` | Selection header and the Contents, Largest Files, and Info tabs. |
| `welcome.rs` | Welcome screen and the scan-in-progress screen. |
| `styles.rs` | Colors, fonts, and widget styles. |
| `icons.rs`, `logo.rs` | Interface icons and the Lorimer mark, drawn on a canvas. |
| `format.rs` | Formatting for sizes, counts, durations, and dates. |
| `layout.rs` | Breakpoints for the wide and stacked layouts. |
| `platform.rs` | Desktop integration iced does not cover, such as the macOS Dock icon. |

**Scanning without blocking.** A scan runs on its own thread. Progress events arrive once per directory, far more often than the interface can use, so a bridging thread forwards a few per second and always forwards the final result. The Cancel button and `Esc` trigger the scan's `ScanCancel`; the scanning screen stays up until the scanner confirms it has stopped.

**Chart layout is a pure function.** `layout_sectors` turns a snapshot and a focused directory into a list of sectors. Drawing, hit testing, and the tests all use that one function, so what you can click always matches what you can see.

**Color.** The first ring takes its colors from a fixed palette in order of size. Deeper rings look up a color in the same palette by hue, drifting away from their parent's hue according to where they sit within it. Colors therefore stay within a family for each top-level folder, and the sidebar reuses the first ring's colors through `chart::child_color`.

**Animation.** The app subscribes to frame ticks only while something is moving: a scan in progress, or the chart sweeping in. When idle it does no work.

### Interface rules

- One stable shell: toolbar with breadcrumbs, chart, sidebar, status bar. On macOS the toolbar is also the title bar.
- Selection is explicit and persistent. Hover adds information without replacing the selection: the chart centre follows the hover, the sidebar header follows the selection.
- Nothing important is available only on hover.
- A panel that can outgrow the window scrolls on its own. The window never scrolls once a scan is shown.
- Wide windows keep the sidebar on the right. Narrow windows stack it below the chart.
- Open and Reveal work for both files and folders.

## Testing

Tests sit next to the code they cover, in a `tests` module at the bottom of each file.

- **Core.** Tests build real directory trees in temporary folders and check totals, hardlink handling, file folding, the largest-files list, progress events, cancellation, the filesystem boundary rules, and volume capacity.
- **Chart.** Tests scan a temporary folder and check sector angles, nesting, hit testing, and that chart and sidebar colors agree.
- **App state.** Tests send messages to `Lorimer::update` and check the result: selection, opening and leaving folders, hover, scan completion, failure, and cancellation, and keyboard shortcuts.
- **Formatting, layout, palette, logo geometry.** Plain unit tests on pure functions.

Drawing code is kept free of decisions, because a decision made while drawing can only be tested by opening a window. There are no screenshot tests yet. The demo tree from `scripts/demo-tree.py` is the intended fixture for them.

CI runs formatting, clippy with warnings as errors, and the tests on macOS, Linux, and Windows, and checks that the workspace builds on the minimum supported Rust version.
