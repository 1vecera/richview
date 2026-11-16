# Documentation build settings

`cargo run --release --locked -p richview-build` stages documentation with `repository_public = false`, `feedback_enabled = false` and the install state read from `assets/install/release.json`.

- Private source: source-install and VSIX build sections are omitted; GitHub links are detached from their labels, including the header icon and feedback issue draft. The build rejects an ungated GitHub hyperlink or clone command. The format vocabulary can still name `gist.github.com` as an allowed embed host; that domain name is not a link. Inert font-license notices retain their original project addresses.
- Public source: `--repository-public` includes source installation and VSIX commands and permits repository/citation links. This flag prepares a build; it does not change repository visibility or publish anything. Daniel must authorize opening the repository separately. Check that the repository's published revision matches the native release before enabling it for delivery.
- Installer off: the docs offer the browser playground and say that the hosted installer is unavailable. The native walkthrough is labeled for an already-installed command.
- Installer on: the native instructions lead with the hosted script, the selected build commit and the PATH line. The source build follows only when the repository-public flag is also enabled. The Marketplace availability sentence stays truthful in either install state.
- Feedback off: a collapsed widget offers a downloadable local note and a no-JavaScript template. There is no Send button or request to the missing service. A public repository additionally permits a reviewed issue draft.
- Feedback on: `--feedback-enabled` adds Send. Enable it only after the same-origin `/api/feedback` route acknowledges stored notes; the page confirms receipt only after `{ok: true}`. This flag does not create or deploy the collector.

The docs use a separate small Rust Wasm entry (`richview-site`'s `docs` binary). It handles copy, search, local feedback and prebuilt style switching. Pages with charts additionally load the report runtime. Text pages do not load the complete site renderer. No new handwritten JavaScript file is used.

The format reference is generated from `docs/RMD-SPEC.md`, split at parsed section headings, with stable field anchors. `/docs/reference.md` keeps the complete contract for agents. Public staging removes maintainer prose and parenthesized lockdown IDs without changing the repository spec or registry.

Chart cards and report previews use sandboxed frames with a shared content-addressed runtime. Standalone downloadable exports remain self-contained. The frame allows scripts and optional downloads, and does not allow same-origin access. The visual owner owns the palette, font, reading measure and final chrome.
