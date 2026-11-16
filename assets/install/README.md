# Install release

`release.json` enables the install guide on `dev`. Use a clean committed release checkout when preparing launch assets. When disabled, the site keeps the coming-soon documentation. The installer defaults to GitHub Releases, but the raw `target/release/install/install.sh` is an unstamped template and cannot install.

On macOS, from the clean release commit, run `cargo run --release --locked -p richview-build -- --repository-public`. On Linux x64, install Docker and the pinned Rust, wasm-bindgen and Node tools, then run `scripts/linux-release/build.sh` from the same clean commit. It runs the same build in Ubuntu 22.04 so the archive requires at most glibc 2.35. The macOS archive goes to `target/release/install/<commit>/`; the Linux archive goes to `target/linux-release-ubuntu-22.04/release/install/<commit>/`. Each has a `.sha256` sidecar. Copy both platform archives to one machine. macOS Apple Silicon and glibc Linux x64 are the supported hosts.

From that same release checkout, assemble the assets with the approved full commit, the exact release tag and the two archive paths:

```sh
node scripts/assemble-release.mjs --commit "$release_commit" --tag "$release_tag" --macos "$macos_archive" --linux "$linux_archive" --output tmp/release-assets
```

The command checks both archive build records, stamps their SHA-256 digests, the approved commit and the tag-specific download URL into `tmp/release-assets/install.sh`, and writes both archives and checksum sidecars beside it. Upload those five files to the same GitHub release only after authorization. Never upload the unstamped template.

After the authorized publication, verify the published bytes against the local assembled installer and approved commit. The verifier reads the exact release URL from that installer:

```sh
node scripts/assemble-release.mjs --verify-published --commit "$release_commit" --installer tmp/release-assets/install.sh
```

The verifier checks the published installer byte for byte, then both archives against the stamped digests and build records, for both the named release and the `latest` URL used by the install command. The site stages only the install guide; it does not host the installer or archives. An enabled assets-only build or unsupported host fails closed. Building and local verification do not publish anything. The production move, deployment and GitHub release require Daniel's explicit authorization.
