# Windows end-to-end walk

ARC-67 ships Windows only with a passing walk on a fresh Windows machine for the candidate's exact commit. The walk runs on Windows Server 2025, the newest Windows AWS offers; Windows 10 and 11 share its kernel and are expected to behave the same, but are untested, because AWS has no Windows 11 image. The full cannot run it, because it needs a Windows machine, so the gate's `windows` step stays `unrun` with that reason. Instead, `merge-prod` refuses a candidate whose `assets/install/release.json` says `"windows": true` until a passing walk for its commit is on its receipt.

## Run it

```sh
tools/windows-e2e/walk.sh RUN COMMIT ARCHIVE EDITOR DATABASE SUMS
node tools/rv-gate/rv-gate.mjs windows candidate/<n> --run RUN
```

- `RUN` is a new folder for the record.
- `COMMIT` is the candidate's full commit.
- `ARCHIVE` is `richview-x86_64-pc-windows-msvc.tar.gz` packed from that commit's Windows build (`richview-build --install-archive … --target x86_64-pc-windows-msvc`).
- `EDITOR` is the same build's `richview-editor.exe`.
- `DATABASE` is the SQL example's `warehouse.duckdb`, which is gitignored: `cargo run --release --locked -p richview-app --example seed_duckdb -- examples/sql-report/data/seed.sql <out>`.
- `SUMS` is the build's sha256 list, one `<sha256>  <name>` line per file, naming `richview.exe`, `richview-sql-worker.exe`, `richview-editor.exe` and `warehouse.duckdb`. The walk refuses an archive whose two programs, or an `EDITOR` or `DATABASE`, differ from it, and checks the editor and database again once they are on the machine. `manifest.json` records every hash and `build.sha256` keeps the list; `rv-gate windows` accepts the run only when each recorded hash is the listed one, and puts the hashes in the receipt note. The installed `richview --version` must name `COMMIT`, which ties the list to the candidate: the full's build manifest cannot, since the full builds no Windows programs.

AWS credentials come from the environment, as the AWS CLI reads them. To try a changed `e2e.ps1` on an existing build, set `WINDOWS_E2E_SCRIPT` to it; `manifest.json` then names the changed script, and `rv-gate windows` does not accept such a run as a record. One run is one `c7i.2xlarge` with Windows Server 2025 in `eu-central-1` for about 6 minutes, roughly 0.08 USD. `AWS_REGION`, `WINDOWS_E2E_TYPE`, `WINDOWS_E2E_IMAGE` and `WINDOWS_E2E_HOURLY_USD` change the defaults.

## What it does

`walk.sh` launches one machine, sends it the candidate and runs `e2e.ps1` there over SSH. The walk:

- installs the published release with the public one-liner, makes a report with it and uninstalls it, keeping the report;
- installs the candidate with `irm http://127.0.0.1:8765/install.ps1 | iex` from a loopback server on the machine, and checks that `richview --version` names the commit;
- records whether the installed files carry a `Zone.Identifier` stream (the Mark of the Web that SmartScreen reads);
- runs `init`, writes a report, then `check` and `generate`, and screenshots the result in Edge, checking that its charts are drawn (headless Edge draws WebGL2 with SwiftShader on a machine without a GPU);
- runs `serve`, checks that the preview reloads, and screenshots it;
- drives `richview-editor.exe` over LSP as the VS Code extension does: diagnostics, the preview and a PDF export;
- exports a PDF through Edge and checks that no browser is left;
- runs the SQL example, checks that its chart is drawn, and checks that a query for a host file is refused with the refusal diagnostic and leaves no worker;
- answers MCP `initialize` and `tools/list`;
- starts `richview server`, publishes a report to it, opens the result and screenshots it;
- uninstalls, keeping reports and settings.

Every step prints `PASS` or `FAIL`, and the log ends in `E2E PASS` or `E2E FAIL`. `RUN` keeps `manifest.json`, `e2e.log`, `out/` with the screenshots and generated files, and the AWS trail.

## Safety net

- **One machine at a time.** Anything tagged `richview=windows-build` stops the run before launch.
- **A time limit on the machine.** It shuts itself down three hours after first boot, and shutting down terminates it.
- **Termination proven at launch.** A dry-run terminate on the real instance id shows the account may end it.
- **Clean-up on any exit.** A trap terminates the machine and waits until it is terminated. It then deletes snapshots of its volume and the security group, and writes the cost into `manifest.json`.

The SSH keys are made fresh for each run and deleted at its end: the key that logs in, and the machine's host key, which the run sends in the machine's user data and pins, so SSH checks every connection against it instead of trusting the first key it is shown. User data is not secret: the instance metadata service serves it to anything on the machine, and `DescribeInstanceAttribute` serves it to the AWS account. That is fine for a host key that lives only as long as one throwaway machine, which the run terminates; never reuse it. (A host key printed to the console output would avoid this, but Windows Server 2025 on these instances has no serial port a script can write to.) Only the run's own address may reach port 22, and the run's files are its user's alone (`umask 077`).
