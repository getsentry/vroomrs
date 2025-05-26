use std::io;

use lz4::{Decoder, EncoderBuilder};

pub(crate) fn decompress_lz4(source: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut decoder = Decoder::new(source)?;
    let mut decoded_data: Vec<u8> = vec![];
    io::copy(&mut decoder, &mut decoded_data)?;
    Ok(decoded_data)
}

pub(crate) fn compress_lz4(source: &mut &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let b: Vec<u8> = vec![];
    let mut encoder = EncoderBuilder::new()
        .block_checksum(lz4::liblz4::BlockChecksum::NoBlockChecksum)
        .level(9)
        .build(b)?;
    io::copy(source, &mut encoder)?;
    let (compressed_data, res) = encoder.finish();
    match res {
        Ok(_) => Ok(compressed_data),
        Err(error) => Err(error),
    }
}
