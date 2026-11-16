//! Compress the report runtime once at build time, without changing its gzip wire format.

use std::io::{self, Read};
use std::num::NonZeroU64;

/// Produce deterministic gzip for the Wasm embedded in native binaries and offline reports.
pub(crate) fn runtime(wasm: &[u8]) -> io::Result<Vec<u8>> {
    let options = zopfli::Options {
        iteration_count: NonZeroU64::new(3).expect("three is nonzero"),
        ..zopfli::Options::default()
    };
    // The no-std encoder uses Zopfli's portable log2, so host libm cannot change the bytes.
    let err = |error: zopfli::Error| io::Error::other(error.to_string());
    let mut encoder =
        zopfli::GzipEncoder::new(options, zopfli::BlockType::Dynamic, Vec::new()).map_err(err)?;
    zopfli::Write::write_all(&mut encoder, wasm).map_err(err)?;
    let compressed = encoder.finish().map_err(err)?;
    let mut decoded = Vec::with_capacity(wasm.len());
    flate2::read::MultiGzDecoder::new(compressed.as_slice())
        .take(wasm.len() as u64 + 1)
        .read_to_end(&mut decoded)?;
    if decoded != wasm {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "compressed report runtime differs from the Wasm",
        ));
    }
    Ok(compressed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn report_runtime_gzip_is_deterministic_and_round_trips() {
        let wasm = b"\0asm\x01\0\0\0data repeated for compression; data repeated for compression";
        let compressed = runtime(wasm).unwrap();
        assert_eq!(runtime(wasm).unwrap(), compressed);
        let mut decoded = Vec::new();
        flate2::read::GzDecoder::new(compressed.as_slice())
            .read_to_end(&mut decoded)
            .unwrap();
        assert_eq!(decoded, wasm);
    }
}
