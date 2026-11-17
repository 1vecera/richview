---
title: Install RichView
description: The native command for glibc Linux x64 and macOS arm64, or the browser playground.
styles: [scientific]
---

RichView supports **Linux x64 with glibc 2.35 or newer and macOS arm64**.<!-- windows-unsupported:start --> Windows and Intel Mac are not supported yet.<!-- windows-unsupported:end --><!-- windows-preview:start --> Windows x64 has a preview without PDF output or SQL data sources; its PowerShell command is on the [install page](https://richview.ai/install). Intel Mac is not supported yet.<!-- windows-preview:end --> You can try the [playground](https://richview.ai/playground) in your browser without installing anything.

On Linux, reports with SQL data sources need kernel 6.2 or newer with Landlock ABI 3 enabled. Debian 12’s default 6.1 kernel and Ubuntu 22.04’s original 5.15 kernel are below this requirement; use a newer kernel for SQL. Reports without SQL need glibc 2.35 or newer and do not have this additional kernel requirement.

## Get the native command

{{native-install}}

The installer creates a new `~/.richview` directory and removes its temporary download files. It will not overwrite an existing directory, modify shell configuration or start a service.

<!-- repository-public:start -->
## Build from source

For contributors, the source build needs Git, [rustup](https://rustup.rs/), Node.js 26.5.0 (pinned in `.node-version`) with npm, and native build tools. On macOS, install Xcode Command Line Tools with `xcode-select --install`; on Linux, install a C compiler and linker through your distribution. The repository pins the Rust toolchain.

A cold build took about 10½ minutes on a 20-core Linux machine with Rust installed. Allow longer on a laptop or when installing prerequisites.

```sh
git clone https://github.com/1vecera/richview.git
cd richview
npm ci --ignore-scripts
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
cargo run --release --locked -p richview-build -- --repository-public
export PATH="$PWD/target/release:$PATH"
richview --help
```

Keep this shell open: the PATH change applies to it. Keep the generated `font-licenses/` beside the executable if you copy it elsewhere. Exported HTML also carries its font notices. The build stages the website and editor files locally.
<!-- repository-public:end -->

## Update

The installer does not overwrite an existing `~/.richview`. To move to the latest release, run the uninstall command below and then the install command again. Your reports and RichView state are kept.

## Uninstall

Run `sh "$HOME/.richview/uninstall.sh" --uninstall --prefix "$HOME/.richview"`. The uninstaller refuses to remove an installation containing changed, missing or extra files. Your reports and RichView state are kept; remove any PATH entry you added yourself.

## Start a report

Once the command is installed, continue with [your first report](https://richview.ai/docs/first-report). For PDF export, you also need a locally installed Chromium, Google Chrome or Microsoft Edge; see [HTML and PDF export](https://richview.ai/docs/export).
