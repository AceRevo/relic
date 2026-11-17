use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::mpsc::sync_channel;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::engine::telemetry::{OperationStatus, StreamTelemetry};

pub const RELIC_MAGIC: &[u8; 8] = b"RELIC01\0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelicHeader {
    pub magic: [u8; 8],
    pub block_size: u32,
    pub total_source_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelicChunkHeader {
    pub chunk_index: u64,
    pub is_sparse: u8,
    pub uncompressed_len: u32,
    pub compressed_len: u32,
    pub blake3_hash: [u8; 32],
}

impl RelicHeader {
    pub fn new(block_size: u32, total_source_bytes: u64) -> Self {
        Self {
            magic: *RELIC_MAGIC,
            block_size,
            total_source_bytes,
        }
    }

    pub fn write_to<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        w.write_all(&self.magic)?;
        w.write_all(&self.block_size.to_le_bytes())?;
        w.write_all(&self.total_source_bytes.to_le_bytes())?;
        Ok(())
    }

    pub fn read_from<R: Read>(r: &mut R) -> std::io::Result<Self> {
        let mut magic = [0u8; 8];
        r.read_exact(&mut magic)?;
        if &magic != RELIC_MAGIC {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid RELIC container magic identifier (corrupted or unrecognized image)",
            ));
        }
        let mut bs_buf = [0u8; 4];
        r.read_exact(&mut bs_buf)?;
        let block_size = u32::from_le_bytes(bs_buf);

        let mut tsb_buf = [0u8; 8];
        r.read_exact(&mut tsb_buf)?;
        let total_source_bytes = u64::from_le_bytes(tsb_buf);

        Ok(Self {
            magic,
            block_size,
            total_source_bytes,
        })
    }
}

impl RelicChunkHeader {
    pub fn write_to<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        w.write_all(&self.chunk_index.to_le_bytes())?;
        w.write_all(&[self.is_sparse])?;
        w.write_all(&self.uncompressed_len.to_le_bytes())?;
        w.write_all(&self.compressed_len.to_le_bytes())?;
        w.write_all(&self.blake3_hash)?;
        Ok(())
    }

    pub fn read_from<R: Read>(r: &mut R) -> std::io::Result<Option<Self>> {
        let mut idx_buf = [0u8; 8];
        match r.read_exact(&mut idx_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e),
        }
        let chunk_index = u64::from_le_bytes(idx_buf);

        let mut sparse_buf = [0u8; 1];
        r.read_exact(&mut sparse_buf)?;
        let is_sparse = sparse_buf[0];

        let mut ulen_buf = [0u8; 4];
        r.read_exact(&mut ulen_buf)?;
        let uncompressed_len = u32::from_le_bytes(ulen_buf);

        let mut clen_buf = [0u8; 4];
        r.read_exact(&mut clen_buf)?;
        let compressed_len = u32::from_le_bytes(clen_buf);

        let mut hash_buf = [0u8; 32];
        r.read_exact(&mut hash_buf)?;

        Ok(Some(Self {
            chunk_index,
            is_sparse,
            uncompressed_len,
            compressed_len,
            blake3_hash: hash_buf,
        }))
    }
}

struct RawCaptureChunk {
    index: u64,
    data: Vec<u8>,
}

struct ProcessedCaptureChunk {
    header: RelicChunkHeader,
    payload: Vec<u8>,
}

/// Captures a storage volume in parallel chunks with multi-threaded compression & BLAKE3 hashing
pub async fn capture_volume(
    source: &Path,
    output: &Path,
    level: i32,
    block_size_mb: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let telemetry = StreamTelemetry::new(0);
    capture_volume_with_telemetry(source, output, level, block_size_mb, Some(telemetry)).await
}

pub async fn capture_volume_with_telemetry(
    source: &Path,
    output: &Path,
    level: i32,
    block_size_mb: usize,
    telemetry: Option<StreamTelemetry>,
) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();
    let block_size = block_size_mb * 1024 * 1024;

    let src_file = File::open(source)?;
    let total_src_len = src_file.metadata().map(|m| m.len()).unwrap_or(0);

    if let Some(ref t) = telemetry {
        t.start(total_src_len, OperationStatus::Streaming);
    }

    let dst_file = File::create(output)?;
    let mut writer = BufWriter::with_capacity(8 * 1024 * 1024, dst_file);

    // Write container header
    let container_header = RelicHeader::new(block_size as u32, total_src_len);
    container_header.write_to(&mut writer)?;

    let worker_count = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .max(2);

    let (raw_tx, raw_rx) = sync_channel::<RawCaptureChunk>(worker_count * 2);
    let (proc_tx, proc_rx) = sync_channel::<ProcessedCaptureChunk>(worker_count * 2);

    let raw_rx_arc = Arc::new(std::sync::Mutex::new(raw_rx));

    // Spawn compression worker threads
    let mut worker_handles = Vec::with_capacity(worker_count);
    for _ in 0..worker_count {
        let rx = Arc::clone(&raw_rx_arc);
        let tx = proc_tx.clone();
        let handle = thread::spawn(move || {
            while let Ok(chunk) = {
                let lock = rx.lock().unwrap();
                lock.recv()
            } {
                let bytes_len = chunk.data.len();
                let is_empty = chunk.data.iter().all(|&b| b == 0);

                if is_empty {
                    let zero_hash = *blake3::hash(&chunk.data).as_bytes();
                    let header = RelicChunkHeader {
                        chunk_index: chunk.index,
                        is_sparse: 1,
                        uncompressed_len: bytes_len as u32,
                        compressed_len: 0,
                        blake3_hash: zero_hash,
                    };
                    if tx
                        .send(ProcessedCaptureChunk {
                            header,
                            payload: Vec::new(),
                        })
                        .is_err()
                    {
                        break;
                    }
                } else {
                    let block_hash = *blake3::hash(&chunk.data).as_bytes();
                    let compressed = match zstd::encode_all(&chunk.data[..], level) {
                        Ok(c) => c,
                        Err(_) => break,
                    };
                    let header = RelicChunkHeader {
                        chunk_index: chunk.index,
                        is_sparse: 0,
                        uncompressed_len: bytes_len as u32,
                        compressed_len: compressed.len() as u32,
                        blake3_hash: block_hash,
                    };
                    if tx
                        .send(ProcessedCaptureChunk {
                            header,
                            payload: compressed,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }
        });
        worker_handles.push(handle);
    }
    drop(proc_tx); // Allow writer to observe EOF when workers exit

    // Spawn reader thread
    let reader_handle = thread::spawn(move || -> std::io::Result<u64> {
        let mut reader = BufReader::with_capacity(block_size, src_file);
        let mut chunk_index: u64 = 0;
        let mut total_bytes: u64 = 0;

        loop {
            let mut buffer = vec![0u8; block_size];
            let mut read_bytes = 0;
            while read_bytes < block_size {
                let n = reader.read(&mut buffer[read_bytes..])?;
                if n == 0 {
                    break;
                }
                read_bytes += n;
            }

            if read_bytes == 0 {
                break;
            }

            buffer.truncate(read_bytes);
            total_bytes += read_bytes as u64;

            if raw_tx
                .send(RawCaptureChunk {
                    index: chunk_index,
                    data: buffer,
                })
                .is_err()
            {
                break;
            }
            chunk_index += 1;
        }
        Ok(total_bytes)
    });

    // Writer: reorders processed chunks sequentially and writes to destination
    let mut reorder_buffer: BTreeMap<u64, ProcessedCaptureChunk> = BTreeMap::new();
    let mut next_expected_index: u64 = 0;
    let mut total_written_active: u64 = 0;
    let mut total_sparse_chunks: u64 = 0;

    for chunk in proc_rx {
        reorder_buffer.insert(chunk.header.chunk_index, chunk);

        while let Some(ready_chunk) = reorder_buffer.remove(&next_expected_index) {
            ready_chunk.header.write_to(&mut writer)?;
            if ready_chunk.header.is_sparse == 0 {
                writer.write_all(&ready_chunk.payload)?;
                total_written_active += 1;
            } else {
                total_sparse_chunks += 1;
            }

            if let Some(ref t) = telemetry {
                t.update(
                    ready_chunk.header.uncompressed_len as u64,
                    ready_chunk.header.is_sparse == 1,
                );
            }

            next_expected_index += 1;
        }
    }

    // Flush any remaining buffered chunks
    while let Some(ready_chunk) = reorder_buffer.remove(&next_expected_index) {
        ready_chunk.header.write_to(&mut writer)?;
        if ready_chunk.header.is_sparse == 0 {
            writer.write_all(&ready_chunk.payload)?;
            total_written_active += 1;
        } else {
            total_sparse_chunks += 1;
        }

        if let Some(ref t) = telemetry {
            t.update(
                ready_chunk.header.uncompressed_len as u64,
                ready_chunk.header.is_sparse == 1,
            );
        }

        next_expected_index += 1;
    }

    writer.flush()?;

    let total_scanned = reader_handle
        .join()
        .map_err(|_| "Reader thread panicked")??;

    for handle in worker_handles {
        let _ = handle.join();
    }

    if let Some(ref t) = telemetry {
        t.finish(true);
    }

    let elapsed = start_time.elapsed().as_secs_f64();
    let speed_mb_s = if elapsed > 0.0 {
        (total_scanned as f64 / 1024.0 / 1024.0) / elapsed
    } else {
        0.0
    };

    println!("[+] Volume snapshot complete.");
    println!(
        "    - Total scanned: {:.2} GB",
        total_scanned as f64 / (1024.0 * 1024.0 * 1024.0)
    );
    println!("    - Total chunks:  {}", next_expected_index);
    println!("    - Active blocks: {}", total_written_active);
    println!("    - Sparse zero-blocks: {}", total_sparse_chunks);
    println!("    - Pipeline throughput: {:.2} MB/s sustained ({:.2}s)", speed_mb_s, elapsed);

    Ok(())
}

struct RawRestoreChunk {
    header: RelicChunkHeader,
    payload: Vec<u8>,
}

struct ProcessedRestoreChunk {
    header: RelicChunkHeader,
    data: Vec<u8>,
}

/// Restores a compressed volume image onto a bare-metal physical target in parallel
pub async fn restore_volume(
    image: &Path,
    target: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let telemetry = StreamTelemetry::new(0);
    restore_volume_with_telemetry(image, target, Some(telemetry)).await
}

pub async fn restore_volume_with_telemetry(
    image: &Path,
    target: &Path,
    telemetry: Option<StreamTelemetry>,
) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();

    let src_file = File::open(image)?;
    let mut reader = BufReader::with_capacity(8 * 1024 * 1024, src_file);

    let container_header = RelicHeader::read_from(&mut reader)?;
    println!(
        "[+] Container verified: Block size: {} MB, Declared uncompressed: {:.2} GB",
        container_header.block_size / (1024 * 1024),
        container_header.total_source_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    );

    if let Some(ref t) = telemetry {
        t.start(container_header.total_source_bytes, OperationStatus::Streaming);
    }

    let dst_file = File::options().write(true).create(true).truncate(false).open(target)?;
    // Pre-set total size so trailing sparse sectors preserve exact target geometry
    if container_header.total_source_bytes > 0 {
        let _ = dst_file.set_len(container_header.total_source_bytes);
    }
    let mut writer = BufWriter::with_capacity(8 * 1024 * 1024, dst_file);

    let worker_count = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .max(2);

    let (raw_tx, raw_rx) = sync_channel::<RawRestoreChunk>(worker_count * 2);
    let (proc_tx, proc_rx) = sync_channel::<ProcessedRestoreChunk>(worker_count * 2);

    let raw_rx_arc = Arc::new(std::sync::Mutex::new(raw_rx));

    // Spawn decompression & verification worker threads
    let mut worker_handles = Vec::with_capacity(worker_count);
    for _ in 0..worker_count {
        let rx = Arc::clone(&raw_rx_arc);
        let tx = proc_tx.clone();
        let handle = thread::spawn(move || {
            while let Ok(chunk) = {
                let lock = rx.lock().unwrap();
                lock.recv()
            } {
                if chunk.header.is_sparse == 1 {
                    if tx
                        .send(ProcessedRestoreChunk {
                            header: chunk.header,
                            data: Vec::new(),
                        })
                        .is_err()
                    {
                        break;
                    }
                } else {
                    let decompressed = match zstd::decode_all(&chunk.payload[..]) {
                        Ok(d) => d,
                        Err(_) => break,
                    };
                    let computed = *blake3::hash(&decompressed).as_bytes();
                    if computed != chunk.header.blake3_hash {
                        eprintln!(
                            "[!] BLAKE3 hash mismatch on chunk #{}",
                            chunk.header.chunk_index
                        );
                        break;
                    }
                    if tx
                        .send(ProcessedRestoreChunk {
                            header: chunk.header,
                            data: decompressed,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }
        });
        worker_handles.push(handle);
    }
    drop(proc_tx);

    // Spawn reader thread
    let reader_handle = thread::spawn(move || -> std::io::Result<u64> {
        let mut total_chunks = 0;
        while let Some(chunk_hdr) = RelicChunkHeader::read_from(&mut reader)? {
            let mut payload = Vec::new();
            if chunk_hdr.is_sparse == 0 {
                payload = vec![0u8; chunk_hdr.compressed_len as usize];
                reader.read_exact(&mut payload)?;
            }
            if raw_tx
                .send(RawRestoreChunk {
                    header: chunk_hdr,
                    payload,
                })
                .is_err()
            {
                break;
            }
            total_chunks += 1;
        }
        Ok(total_chunks)
    });

    let mut reorder_buffer: BTreeMap<u64, ProcessedRestoreChunk> = BTreeMap::new();
    let mut next_expected_index: u64 = 0;
    let mut written_bytes: u64 = 0;
    let mut restored_chunks: u64 = 0;
    let mut sparse_chunks: u64 = 0;

    for chunk in proc_rx {
        reorder_buffer.insert(chunk.header.chunk_index, chunk);

        while let Some(ready_chunk) = reorder_buffer.remove(&next_expected_index) {
            if ready_chunk.header.is_sparse == 1 {
                sparse_chunks += 1;
                if let Err(_) = writer.seek(SeekFrom::Current(ready_chunk.header.uncompressed_len as i64)) {
                    let zero_buf = vec![0u8; ready_chunk.header.uncompressed_len as usize];
                    writer.write_all(&zero_buf)?;
                }
                written_bytes += ready_chunk.header.uncompressed_len as u64;
            } else {
                writer.write_all(&ready_chunk.data)?;
                written_bytes += ready_chunk.data.len() as u64;
                restored_chunks += 1;
            }

            if let Some(ref t) = telemetry {
                t.update(
                    ready_chunk.header.uncompressed_len as u64,
                    ready_chunk.header.is_sparse == 1,
                );
            }

            next_expected_index += 1;
        }
    }

    while let Some(ready_chunk) = reorder_buffer.remove(&next_expected_index) {
        if ready_chunk.header.is_sparse == 1 {
            sparse_chunks += 1;
            if let Err(_) = writer.seek(SeekFrom::Current(ready_chunk.header.uncompressed_len as i64)) {
                let zero_buf = vec![0u8; ready_chunk.header.uncompressed_len as usize];
                writer.write_all(&zero_buf)?;
            }
            written_bytes += ready_chunk.header.uncompressed_len as u64;
        } else {
            writer.write_all(&ready_chunk.data)?;
            written_bytes += ready_chunk.data.len() as u64;
            restored_chunks += 1;
        }

        if let Some(ref t) = telemetry {
            t.update(
                ready_chunk.header.uncompressed_len as u64,
                ready_chunk.header.is_sparse == 1,
            );
        }

        next_expected_index += 1;
    }

    writer.flush()?;

    let _ = reader_handle.join();
    for handle in worker_handles {
        let _ = handle.join();
    }

    if let Some(ref t) = telemetry {
        t.finish(true);
    }

    let elapsed = start_time.elapsed().as_secs_f64();
    let speed_mb_s = if elapsed > 0.0 {
        (written_bytes as f64 / 1024.0 / 1024.0) / elapsed
    } else {
        0.0
    };

    println!("[+] Target reanimation finished.");
    println!(
        "    - Restored data: {:.2} GB",
        written_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    );
    println!("    - Active chunks written: {}", restored_chunks);
    println!("    - Sparse chunks aligned: {}", sparse_chunks);
    println!("    - Transfer rate: {:.2} MB/s ({:.2}s)", speed_mb_s, elapsed);

    Ok(())
}
