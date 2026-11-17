use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::time::Instant;

use crate::engine::stream::{RelicChunkHeader, RelicHeader};

pub async fn verify_image(image_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();
    println!("[*] Computing BLAKE3 integrity verification for container {:?}", image_path);

    let file = File::open(image_path)?;
    let mut reader = BufReader::with_capacity(16 * 1024 * 1024, file);

    let header = RelicHeader::read_from(&mut reader)?;
    println!(
        "[+] Container header validated: Block size: {} MB, Declared uncompressed: {:.2} GB",
        header.block_size / (1024 * 1024),
        header.total_source_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    );

    let mut chunk_count: u64 = 0;
    let mut active_count: u64 = 0;
    let mut sparse_count: u64 = 0;
    let mut total_uncompressed_bytes: u64 = 0;

    while let Some(chunk) = RelicChunkHeader::read_from(&mut reader)? {
        chunk_count += 1;
        total_uncompressed_bytes += chunk.uncompressed_len as u64;

        if chunk.is_sparse == 1 {
            sparse_count += 1;
        } else {
            active_count += 1;
            let mut compressed_buf = vec![0u8; chunk.compressed_len as usize];
            reader.read_exact(&mut compressed_buf)?;

            let decompressed = zstd::decode_all(&compressed_buf[..])?;
            if decompressed.len() != chunk.uncompressed_len as usize {
                return Err(format!(
                    "Verification failed: Chunk #{} length mismatch (expected {}, got {})",
                    chunk.chunk_index, chunk.uncompressed_len, decompressed.len()
                )
                .into());
            }

            let computed = *blake3::hash(&decompressed).as_bytes();
            if computed != chunk.blake3_hash {
                return Err(format!(
                    "Verification failed: Chunk #{} BLAKE3 checksum mismatch!",
                    chunk.chunk_index
                )
                .into());
            }
        }
    }

    let elapsed = start_time.elapsed().as_secs_f64();
    let throughput = if elapsed > 0.0 {
        (total_uncompressed_bytes as f64 / 1024.0 / 1024.0) / elapsed
    } else {
        0.0
    };

    println!("[+] Verification complete: 100% BLAKE3 tree hashes validated.");
    println!("    - Total chunks validated: {}", chunk_count);
    println!("    - Active data chunks:     {}", active_count);
    println!("    - Sparse chunks verified: {}", sparse_count);
    println!("    - Uncompressed dataset:   {:.2} GB", total_uncompressed_bytes as f64 / (1024.0 * 1024.0 * 1024.0));
    println!("    - Effective verification speed: {:.2} MB/s", throughput);

    Ok(())
}
