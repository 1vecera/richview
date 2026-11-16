# Observe the current pie during motion verification

The reduced-motion verifier intermittently failed even though the pie had reached its complete geometry. In a retained failure on unchanged main, both sampled paths were detached from the document; their computed transforms were empty strings, while the live chart contained two replacement marks. The selector and `evaluateAll` callback could straddle a scheduled SVG replacement.

The check now resolves the stable figure and queries its current marks inside one evaluation callback. It also asserts that the sampled marks remain connected to that figure. The immediate path and transform assertions, the existing 200 ms unchanged-path check, lazy mounting, synchronous print, facet settlement and failed-mount checks are retained. No retry, extra wait or product change is added.

`verification.json` records the failing baseline observation and twenty consecutive successful lifecycle runs with the repaired snapshot. Native and browser product source is unchanged, and the fresh Web Wasm hash is byte-identical to the main baseline. Run `node scripts/verify-motion-runtime.mjs` after the standard release build to exercise the complete lifecycle check; it writes its actual observations below `tmp/verification/motion-runtime-*`.

The current delivery is verified against main `bb8b112c121eae1ec70d810df1b5e6a2070c9c4e` at product/test head `7881a56a1f65efe8433dce81838882ada8111a11`. Its fresh Web Wasm remains byte-identical to that new baseline. Five further owner lifecycle runs, one independent lifecycle run and the complete required Rust/build/host suite pass. The original twenty-run gate and detached-node failure remain explicitly historical in the verification record.
