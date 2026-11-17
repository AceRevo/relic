use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Instant;

pub const RELIC_MAGIC: &[u8; 8] = b"RELIC01\0";

#[derive(Debug, Clone)]
pub struct RelicHeader {
    pub magic: [u8; 8],
    pub block_size: u32,
    pub total_source_bytes: u64,
}

#[derive(Debug, Clone)]
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

/// Captures a storage volume in sparse chunks, compressing active blocks with Zstandard
pub async fn capture_volume(
    source: &Path,
    output: &Path,
    level: i32,
    block_size_mb: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();
    let block_size = block_size_mb * 1024 * 1024;
    println!("[+] Allocating {} MB sparse streaming buffer...", block_size_mb);

    let src_file = File::open(source)?;
    let total_src_len = src_file.metadata().map(|m| m.len()).unwrap_or(0);
    let mut reader = BufReader::with_capacity(block_size, src_file);

    let dst_file = File::create(output)?;
    let mut writer = BufWriter::with_capacity(8 * 1024 * 1024, dst_file);

    // Write container header
    let header = RelicHeader::new(block_size as u32, total_src_len);
    header.write_to(&mut writer)?;

    let mut buffer = vec![0u8; block_size];
    let mut total_bytes_read: u64 = 0;
    let mut active_blocks: u64 = 0;
    let mut skipped_sparse_blocks: u64 = 0;
    let mut chunk_index: u64 = 0;

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }

        total_bytes_read += bytes_read as u64;

        // Check if block is entirely null bytes (sparse detection)
        let is_empty = buffer[..bytes_read].iter().all(|&b| b == 0);

        if is_empty {
            skipped_sparse_blocks += 1;
            let zero_hash = *blake3::hash(&buffer[..bytes_read]).as_bytes();
            let chunk_hdr = RelicChunkHeader {
                chunk_index,
                is_sparse: 1,
                uncompressed_len: bytes_read as u32,
                compressed_len: 0,
                blake3_hash: zero_hash,
            };
            chunk_hdr.write_to(&mut writer)?;
        } else {
            active_blocks += 1;
            let block_hash = *blake3::hash(&buffer[..bytes_read]).as_bytes();
            let compressed = zstd::encode_all(&buffer[..bytes_read], level)?;

            let chunk_hdr = RelicChunkHeader {
                chunk_index,
                is_sparse: 0,
                uncompressed_len: bytes_read as u32,
                compressed_len: compressed.len() as u32,
                blake3_hash: block_hash,
            };
            chunk_hdr.write_to(&mut writer)?;
            writer.write_all(&compressed)?;
        }

        chunk_index += 1;
    }

    writer.flush()?;
    let elapsed = start_time.elapsed().as_secs_f64();
    let speed_mb_s = if elapsed > 0.0 {
        (total_bytes_read as f64 / 1024.0 / 1024.0) / elapsed
    } else {
        0.0
    };

    println!("[+] Volume snapshot complete.");
    println!("    - Total scanned: {:.2} GB", total_bytes_read as f64 / (1024.0 * 1024.0 * 1024.0));
    println!("    - Total chunks:  {}", chunk_index);
    println!("    - Active blocks: {}", active_blocks);
    println!("    - Sparse zero-blocks skipped: {}", skipped_sparse_blocks);
    println!("    - Time elapsed: {:.2}s ({:.2} MB/s sustained)", elapsed, speed_mb_s);

    Ok(())
}

/// Restores a compressed volume image onto a bare-metal physical target
pub async fn restore_volume(
    image: &Path,
    target: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();
    println!("[+] Reading image container {:?}", image);

    let src_file = File::open(image)?;
    let mut reader = BufReader::with_capacity(8 * 1024 * 1024, src_file);

    let header = RelicHeader::read_from(&mut reader)?;
    println!(
        "[+] Container verified: Block size: {} MB, Declared source: {:.2} GB",
        header.block_size / (1024 * 1024),
        header.total_source_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    );

    let dst_file = File::options().write(true).create(true).truncate(false).open(target)?;
    let mut writer = BufWriter::with_capacity(8 * 1024 * 1024, dst_file);

    let mut written_bytes: u64 = 0;
    let mut restored_chunks: u64 = 0;
    let mut sparse_chunks: u64 = 0;

    while let Some(chunk) = RelicChunkHeader::read_from(&mut reader)? {
        if chunk.is_sparse == 1 {
            sparse_chunks += 1;
            // Attempt seek forward to preserve sparseness; fall back to zero writing if unsupported
            if let Err(_) = writer.seek(SeekFrom::Current(chunk.uncompressed_len as i64)) {
                let zero_buf = vec![0u8; chunk.uncompressed_len as usize];
                writer.write_all(&zero_buf)?;
            }
            written_bytes += chunk.uncompressed_len as u64;
        } else {
            let mut compressed_buf = vec![0u8; chunk.compressed_len as usize];
            reader.read_exact(&mut compressed_buf)?;

            let decompressed = zstd::decode_all(&compressed_buf[..])?;
            if decompressed.len() != chunk.uncompressed_len as usize {
                return Err(format!(
                    "Decompressed size mismatch at chunk #{}: expected {}, got {}",
                    chunk.chunk_index, chunk.uncompressed_len, decompressed.len()
                )
                .into());
            }

            let computed_hash = *blake3::hash(&decompressed).as_bytes();
            if computed_hash != chunk.blake3_hash {
                return Err(format!(
                    "Integrity check failed at chunk #{}: BLAKE3 mismatch",
                    chunk.chunk_index
                )
                .into());
            }

            writer.write_all(&decompressed)?;
            written_bytes += decompressed.len() as u64;
            restored_chunks += 1;
        }
    }

    writer.flush()?;
    let elapsed = start_time.elapsed().as_secs_f64();
    let speed_mb_s = if elapsed > 0.0 {
        (written_bytes as f64 / 1024.0 / 1024.0) / elapsed
    } else {
        0.0
    };

    println!("[+] Target reanimation finished.");
    println!("    - Restored: {:.2} GB", written_bytes as f64 / (1024.0 * 1024.0 * 1024.0));
    println!("    - Active chunks written: {}", restored_chunks);
    println!("    - Sparse chunks aligned: {}", sparse_chunks);
    println!("    - Transfer rate: {:.2} MB/s", speed_mb_s);

    Ok(())
}
