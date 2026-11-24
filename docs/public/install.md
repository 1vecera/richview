---
title: Install RichView
description: Install, update or remove the native command, or try the browser playground.
styles: [scientific]
---

RichView supports **Linux x64 with glibc 2.35 or newer and macOS arm64**.<!-- windows-unsupported:start --> Windows and Intel Mac are not supported yet.<!-- windows-unsupported:end --><!-- windows-preview:start --> It also runs on **Windows x64** with the same features, from its own PowerShell command: it is tested on Windows Server 2025, and Windows 10 and 11 on x64 should behave the same but are untested; see [Windows](https://richview.ai/docs/install#windows) below. Intel Mac is not supported yet.<!-- windows-preview:end --> You can try the [playground](https://richview.ai/playground) in your browser without installing anything.

On Linux, SQL queries and Parquet files need kernel 6.2 or newer with Landlock ABI 3 enabled. Debian 12’s default 6.1 kernel and Ubuntu 22.04’s original 5.15 kernel are below this requirement. Reports using neither SQL nor Parquet need glibc 2.35 or newer and have no additional kernel requirement.

## Get the native command

{{native-install}}

The installer creates a new `~/.richview` directory and removes its temporary download files. It will not overwrite an existing directory, modify shell configuration or start a service.

Publishing goes to RichView Cowork, the report server: RichView hosts it at [app.richview.ai](https://app.richview.ai), or you run your own with `richview server`. See [publishing](https://richview.ai/docs/publishing).
<!-- windows-preview:start -->

## Windows

The Windows command has the same features as the macOS and Linux one: the local preview, the MCP server, PDF output and SQL data sources.

### Install in PowerShell

{{windows-install}}

The installer verifies the archive and installs into a new `.richview` folder in your home folder. It changes neither your PATH nor the registry and starts no service. Run the command by its full path, such as `& "$HOME\.richview\bin\richview.exe" init first-report`, or add `$HOME\.richview\bin` to your PATH yourself.

- **What is tested.** Before a release ships Windows, its exact build is installed on a fresh Windows Server 2025 machine and walked end to end: the install, the update from the previous release, `init`, `check` and `generate`, the local preview, the VS Code extension's editor service, PDF output, SQL data sources, the MCP server, publishing to a RichView server, and the uninstall. Windows 10 and 11 share Server 2025's kernel and should behave the same, but have not been tested.
- **Not signed yet.** The programs carry no code signature. The PowerShell command installs them without the download mark that SmartScreen checks. On Windows 11 with Smart App Control turned on, unsigned programs may be blocked; that has not been tested.
- **PDF output** prints through Microsoft Edge, which comes with Windows, or through Chrome or Chromium; see [HTML and PDF export](https://richview.ai/docs/export).
- **SQL data sources** run in a confined worker, as on Linux and macOS. The [data guide](https://richview.ai/docs/data#query-through-a-native-connector) says how Windows copies the database for it.
- **Update** by uninstalling and running the install command again. **Uninstall** with `powershell -ExecutionPolicy Bypass -File "$HOME\.richview\uninstall.ps1" -Uninstall`. Your reports and RichView state are kept; remove any PATH entry you added yourself.
<!-- windows-preview:end -->

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

Keep this shell open: the PATH change applies to it. If you copy `richview` elsewhere, keep the SQL and Parquet worker and font notices beside it:

```text
bin/
  richview
  richview-sql-worker
  font-licenses/
```

Exported HTML also carries its font notices. The build stages the website and editor files locally.
<!-- repository-public:end -->

## Update

The installer does not overwrite an existing `~/.richview`. To move to the latest release, run the uninstall command below and then the install command again. Your reports and RichView state are kept.<!-- windows-preview:start --> On Windows, follow [Windows](https://richview.ai/docs/install#windows) above.<!-- windows-preview:end -->

## Uninstall

Run `sh "$HOME/.richview/uninstall.sh" --uninstall --prefix "$HOME/.richview"`. The uninstaller refuses to remove an installation containing changed, missing or extra files. Your reports and RichView state are kept; remove any PATH entry you added yourself.

## Start a report

Once the command is installed, continue with [your first report](https://richview.ai/docs/first-report). For PDF export, you also need a locally installed Chromium, Google Chrome or Microsoft Edge; see [HTML and PDF export](https://richview.ai/docs/export).
