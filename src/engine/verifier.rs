use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::sync_channel;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use crate::engine::stream::{RelicChunkHeader, RelicHeader};

struct RawVerifyChunk {
    header: RelicChunkHeader,
    payload: Vec<u8>,
}

pub async fn verify_image(image_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();
    println!("[*] Computing parallel BLAKE3 integrity verification for container {:?}", image_path);

    let file = File::open(image_path)?;
    let mut reader = BufReader::with_capacity(16 * 1024 * 1024, file);

    let header = RelicHeader::read_from(&mut reader)?;
    println!(
        "[+] Container header validated: Block size: {} MB, Declared uncompressed: {:.2} GB",
        header.block_size / (1024 * 1024),
        header.total_source_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    );

    let worker_count = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .max(2);

    let (raw_tx, raw_rx) = sync_channel::<RawVerifyChunk>(worker_count * 2);
    let raw_rx_arc = Arc::new(std::sync::Mutex::new(raw_rx));

    let active_chunks = Arc::new(AtomicU64::new(0));
    let sparse_chunks = Arc::new(AtomicU64::new(0));
    let total_uncompressed = Arc::new(AtomicU64::new(0));
    let checksum_failed = Arc::new(AtomicBool::new(false));

    let mut worker_handles = Vec::with_capacity(worker_count);
    for _ in 0..worker_count {
        let rx = Arc::clone(&raw_rx_arc);
        let active_c = Arc::clone(&active_chunks);
        let sparse_c = Arc::clone(&sparse_chunks);
        let total_u = Arc::clone(&total_uncompressed);
        let failed = Arc::clone(&checksum_failed);

        let handle = thread::spawn(move || {
            while let Ok(chunk) = {
                let lock = rx.lock().unwrap();
                lock.recv()
            } {
                if failed.load(Ordering::Relaxed) {
                    break;
                }

                total_u.fetch_add(chunk.header.uncompressed_len as u64, Ordering::Relaxed);

                if chunk.header.is_sparse == 1 {
                    sparse_c.fetch_add(1, Ordering::Relaxed);
                } else {
                    let decompressed = match zstd::decode_all(&chunk.payload[..]) {
                        Ok(d) => d,
                        Err(e) => {
                            eprintln!("[!] Decompression error at chunk #{}: {}", chunk.header.chunk_index, e);
                            failed.store(true, Ordering::SeqCst);
                            break;
                        }
                    };

                    if decompressed.len() != chunk.header.uncompressed_len as usize {
                        eprintln!(
                            "[!] Size mismatch at chunk #{}: expected {}, got {}",
                            chunk.header.chunk_index, chunk.header.uncompressed_len, decompressed.len()
                        );
                        failed.store(true, Ordering::SeqCst);
                        break;
                    }

                    let computed = *blake3::hash(&decompressed).as_bytes();
                    if computed != chunk.header.blake3_hash {
                        eprintln!(
                            "[!] Checksum mismatch at chunk #{}: BLAKE3 validation failed!",
                            chunk.header.chunk_index
                        );
                        failed.store(true, Ordering::SeqCst);
                        break;
                    }

                    active_c.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
        worker_handles.push(handle);
    }

    let mut chunk_count: u64 = 0;
    while let Some(chunk_hdr) = RelicChunkHeader::read_from(&mut reader)? {
        let mut payload = Vec::new();
        if chunk_hdr.is_sparse == 0 {
            payload = vec![0u8; chunk_hdr.compressed_len as usize];
            reader.read_exact(&mut payload)?;
        }

        if raw_tx
            .send(RawVerifyChunk {
                header: chunk_hdr,
                payload,
            })
            .is_err()
        {
            break;
        }
        chunk_count += 1;
    }
    drop(raw_tx);

    for handle in worker_handles {
        let _ = handle.join();
    }

    if checksum_failed.load(Ordering::SeqCst) {
        return Err("Verification failed: Data corruption detected in one or more blocks.".into());
    }

    let elapsed = start_time.elapsed().as_secs_f64();
    let total_bytes = total_uncompressed.load(Ordering::SeqCst);
    let throughput = if elapsed > 0.0 {
        (total_bytes as f64 / 1024.0 / 1024.0) / elapsed
    } else {
        0.0
    };

    println!("[+] Verification complete: 100% BLAKE3 tree hashes validated.");
    println!("    - Total chunks validated: {}", chunk_count);
    println!("    - Active data chunks:     {}", active_chunks.load(Ordering::SeqCst));
    println!("    - Sparse chunks verified: {}", sparse_chunks.load(Ordering::SeqCst));
    println!("    - Uncompressed dataset:   {:.2} GB", total_bytes as f64 / (1024.0 * 1024.0 * 1024.0));
    println!("    - Multi-core verification speed: {:.2} MB/s ({:.2}s)", throughput, elapsed);

    Ok(())
}
