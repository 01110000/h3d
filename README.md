# H3D

A single-codebase [Dioxus Native](https://github.com/DioxusLabs/dioxus/tree/main/packages/native)
app (Blitz + wgpu, no webview) for **Windows, Linux, macOS, Android and iOS**.

At startup it displays one thing: an interactive **3D Gaussian Splatting (3DGS)** scene of a
white torus on a black background.

| Input | Desktop | Mobile |
| --- | --- | --- |
| Rotate | drag with the mouse | drag with one finger |
| Zoom in / out | mouse wheel / trackpad scroll | pinch with two fingers |

## How it works

- `src/splats.rs` procedurally generates 40,000 anisotropic gaussians uniformly over the torus
  surface (flat discs in the tangent plane, lighting baked into their colour, like a captured
  3DGS scene).
- `src/shader.wgsl` is a standard 3DGS rasterizer: each gaussian's 3D covariance is projected
  to a 2D screen-space ellipse (EWA splatting, `Σ' = J W Σ Wᵀ Jᵀ`), drawn as an instanced quad
  and alpha-composited back to front with premultiplied alpha.
- `src/renderer.rs` owns the wgpu pipeline and depth-sorts the splats on the CPU (16-bit
  counting sort) whenever the view changes. Only vertex/uniform buffers are used, so it runs on
  Vulkan, Metal, DX12 and GLES.
- `src/widget.rs` is a Blitz custom `Widget`: Dioxus Native hands it its wgpu device, it renders
  into an offscreen texture that Blitz composites into the page, and it receives the pointer /
  wheel events for the trackball rotation and zoom.
- `src/main.rs` is the Dioxus app: one full-viewport `<object>` hosting the widget.

## Prerequisites

- Rust (stable) and the Dioxus CLI matching the Dioxus version used here:
  `cargo install dioxus-cli --version 0.8.0-alpha.3 --locked`
- Linux: `libfontconfig1-dev libxkbcommon-dev libwayland-dev libx11-dev pkg-config` and a
  Vulkan or GL driver.
- Android: Android SDK + NDK (`ANDROID_HOME`, `ANDROID_NDK_HOME`), Java 17,
  `rustup target add aarch64-linux-android x86_64-linux-android`.
- iOS / macOS: Xcode, `rustup target add aarch64-apple-ios aarch64-apple-ios-sim`.

## Run

```sh
# Desktop (Windows, Linux, macOS) – on the host OS
dx serve            # or: cargo run --release

# Android (device or emulator)
dx serve --android

# iOS (device or simulator)
dx serve --ios
```

## Packages

Pushing a `v*` tag runs `.github/workflows/release.yml`. It builds one-click packages on each platform's
native runner and attaches them to a GitHub Release (pull requests and manual runs upload them as workflow artifacts):

| Platform | Package |
| --- | --- |
| Windows x86_64 | `.msi` installer and `.exe` (NSIS) installer |
| macOS (Apple silicon / Intel) | `.dmg` |
| Linux x86_64 | `.AppImage` and `.deb` |
| Android arm64 | `.apk` (signed with the debug key) |
| iOS | `.ipa` (unsigned: needs re-signing with an Apple Developer certificate to install on a device) |

The Windows and macOS packages are not code-signed, so SmartScreen and Gatekeeper warn on first launch.

Local equivalent: `dx bundle --release --renderer native --linux --package-types appimage --package-types deb`
(swap the platform flag and package types; pass `--renderer native`, otherwise desktop targets default to the webview renderer).
