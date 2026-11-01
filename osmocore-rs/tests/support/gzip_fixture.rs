//! Shared reader for fixtures stored gzip-compressed. Fingerprints and hashes
//! apply to the decompressed text, so compression doesn't change the evidence.
use flate2::bufread::GzDecoder;
use std::io::Read;

/// Decompress a complete fixture, rejecting corrupt data and trailing bytes.
pub fn try_text(compressed: &[u8]) -> Result<String, String> {
    let mut decoder = GzDecoder::new(compressed);
    let mut text = String::new();
    decoder
        .read_to_string(&mut text)
        .map_err(|error| error.to_string())?;
    if decoder.into_inner().is_empty() {
        Ok(text)
    } else {
        Err("trailing bytes after the gzip stream".to_owned())
    }
}

pub fn text(compressed: &[u8]) -> String {
    try_text(compressed).expect("committed fixture must decompress")
}
