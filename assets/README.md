# Brand assets

The mark is the app's sunburst rings bent into an L: a chart and a street corner at once.

| File | Use |
| --- | --- |
| `logo/icon.svg` | App icon master (1024 px canvas, macOS icon grid). Source for every raster icon. |
| `logo/mark.svg` | The mark alone on a transparent background. |
| `logo/logo.svg` | Icon and wordmark for light backgrounds. |
| `logo/logo-dark.svg` | Icon and wordmark for dark backgrounds. |
| `icons/lorimer.icns` | macOS app bundle icon. |
| `icons/lorimer.ico` | Windows icon (16 to 256 px). Not yet embedded in `lorimer.exe`. |
| `icons/png/icon-<size>.png` | PNGs from 16 to 1024 px, for Linux desktop entries and the web. |
| `icons/window-icon-128.rgba` | Raw pixels embedded in the binary as the window icon on Linux and Windows. |
| `linux/lorimer.desktop` | Desktop entry shipped in the Linux download. |

The wordmark is outlined, so the logos need no font installed.

Everything under `icons/` is generated. To change the icon, edit `logo/icon.svg` and run:

```bash
assets/build-icons.sh
```

The script needs `rsvg-convert` (librsvg) and ImageMagick, plus `iconutil` on macOS for the `.icns`. The app also draws the mark itself in `crates/gui-iced/src/logo.rs`; keep that in step with `logo/mark.svg`.

## Colors

| | Hex |
| --- | --- |
| Blue | `#409CFF` |
| Purple | `#BF6EFA` |
| Pink | `#FF6496` |
| Icon background | `#2A2B33` to `#131418`, top to bottom |
