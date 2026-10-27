//! GDeflate tile streams (DirectStorage format), used for packed texture
//! surfaces in HashFS v2.
//!
//! Stream layout: an 8-byte header (`id = 4`, `magic = !id`, `u16 tile
//! count`, `u32` bitfield with the tile-size index in bits 0..2 and the last
//! tile's uncompressed size in bits 2..20), a table of `u32` tile offsets,
//! then the compressed tiles. Entry 0 of the table holds the compressed size
//! of the *last* tile, as tile 0 always starts at offset 0.

const STREAM_ID: u8 = 4;
const HEADER_LEN: usize = 8;
const TILE_SIZE: u32 = 64 * 1024;

/// Decompresses a GDeflate tile stream, validating every size and offset
/// before handing tiles to the decoder.
pub(crate) fn decompress(stream: &[u8]) -> Result<Vec<u8>, String> {
    let header = stream
        .get(..HEADER_LEN)
        .ok_or("stream shorter than header")?;
    let (id, magic) = (header[0], header[1]);
    if id != STREAM_ID || magic != !id {
        return Err(format!("bad stream id {id:#04x}/{magic:#04x}"));
    }
    let tile_count = usize::from(u16::from_le_bytes([header[2], header[3]]));
    let bits = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
    let last_tile_size = (bits >> 2) & 0x3_ffff;
    if tile_count == 0 {
        return Ok(Vec::new());
    }

    let table_end = HEADER_LEN + tile_count * 4;
    let table = stream
        .get(HEADER_LEN..table_end)
        .ok_or("truncated tile table")?;
    let offsets: Vec<usize> = table
        .chunks_exact(4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
        .collect();
    let data = &stream[table_end..];

    let mut tiles = Vec::with_capacity(tile_count);
    for index in 0..tile_count {
        let start = if index == 0 { 0 } else { offsets[index] };
        let compressed = if index + 1 < tile_count {
            offsets[index + 1]
                .checked_sub(start)
                .ok_or("tile offsets are not increasing")?
        } else {
            offsets[0]
        };
        if start + compressed > data.len() {
            return Err(format!("tile {index} exceeds stream"));
        }
        let uncompressed = if index + 1 == tile_count && last_tile_size != 0 {
            last_tile_size
        } else {
            TILE_SIZE
        };
        tiles.push((start, compressed, uncompressed));
    }

    // `gdeflate::Decompressor` reads tiles back to back from the start of
    // the buffer; verify the stream is laid out that way.
    let mut expected_start = 0;
    let mut wrapped_tiles = Vec::with_capacity(tile_count);
    for &(start, compressed, uncompressed) in &tiles {
        if start != expected_start {
            return Err("tiles are not contiguous".to_owned());
        }
        expected_start += compressed;
        wrapped_tiles.push(gdeflate::Tile {
            compressed_size: compressed as u32,
            uncompressed_size: uncompressed,
        });
    }

    let mut decompressor =
        gdeflate::Decompressor::new().map_err(|e| format!("gdeflate init: {e}"))?;
    decompressor
        .decompress(gdeflate::CompressionResult::new(
            &data[..expected_start],
            &wrapped_tiles,
            TILE_SIZE,
        ))
        .map_err(|e| format!("gdeflate: {e}"))
}

#[cfg(test)]
mod tests {
    use super::decompress;

    #[test]
    fn rejects_bad_stream_id() {
        assert!(decompress(&[5, !5, 1, 0, 0, 0, 0, 0]).is_err());
    }

    #[test]
    fn rejects_tile_extending_past_stream() {
        // One tile claiming 100 compressed bytes with only 2 available.
        let mut stream = vec![4, !4u8, 1, 0, 0, 0, 0, 0];
        stream.extend(100u32.to_le_bytes());
        stream.extend([0, 0]);
        assert!(decompress(&stream).is_err());
    }

    #[test]
    fn round_trips_data_compressed_by_reference_encoder() {
        let input: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let mut compressor =
            gdeflate::Compressor::new(gdeflate::CompressionLevel::Level6).expect("compressor");
        let compressed = compressor.compress(&input, 64 * 1024).expect("compress");

        // Assemble a DirectStorage tile stream around the compressed tiles.
        let tile_count = compressed.tiles.len();
        let last = compressed.tiles.last().expect("tiles").uncompressed_size;
        let mut stream = vec![4, !4u8];
        stream.extend((tile_count as u16).to_le_bytes());
        stream.extend(((last % (64 * 1024)) << 2).to_le_bytes());
        let mut offsets = vec![compressed.tiles[tile_count - 1].compressed_size];
        let mut running = 0;
        for tile in &compressed.tiles[..tile_count - 1] {
            running += tile.compressed_size;
            offsets.push(running);
        }
        for offset in offsets {
            stream.extend(offset.to_le_bytes());
        }
        stream.extend(&compressed.bytes);

        assert_eq!(decompress(&stream).expect("decompress"), input);
    }
}
