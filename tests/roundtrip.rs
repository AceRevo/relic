use std::fs::File;
use std::io::{Read, Write};

// Include module functions directly for integration testing
#[tokio::test]
async fn test_synthetic_disk_sparse_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = std::env::temp_dir().join("relic_test_roundtrip");
    let _ = std::fs::create_dir_all(&temp_dir);

    let src_path = temp_dir.join("synthetic_disk.raw");
    let relic_path = temp_dir.join("synthetic_disk.relic");
    let restored_path = temp_dir.join("synthetic_disk_restored.raw");

    // Clean up previous run if any
    let _ = std::fs::remove_file(&src_path);
    let _ = std::fs::remove_file(&relic_path);
    let _ = std::fs::remove_file(&restored_path);

    // 1. Create a 4 MB synthetic disk with mixed sparse and active blocks
    // Block 0: Active pattern (1 MB)
    // Block 1: Sparse zeroes (1 MB)
    // Block 2: Active pattern (1 MB)
    // Block 3: Sparse zeroes (1 MB)
    let block_size = 1024 * 1024; // 1 MB
    {
        let mut src_file = File::create(&src_path)?;
        let active_block_1 = vec![0x42u8; block_size];
        let sparse_block = vec![0x00u8; block_size];
        let active_block_2 = vec![0x7Fu8; block_size];

        src_file.write_all(&active_block_1)?;
        src_file.write_all(&sparse_block)?;
        src_file.write_all(&active_block_2)?;
        src_file.write_all(&sparse_block)?;
        src_file.flush()?;
    }

    let mut original_bytes = Vec::new();
    File::open(&src_path)?.read_to_end(&mut original_bytes)?;
    let original_hash = blake3::hash(&original_bytes);

    // 2. Capture into RELIC container
    relic_engine_capture(&src_path, &relic_path, 1, 1).await?;
    assert!(relic_path.exists(), "Engram container file must exist");

    // 3. Verify RELIC container integrity
    relic_engine_verify(&relic_path).await?;

    // 4. Restore onto target disk
    relic_engine_restore(&relic_path, &restored_path).await?;
    assert!(restored_path.exists(), "Restored disk file must exist");

    // 5. Compare byte-by-byte
    let mut restored_bytes = Vec::new();
    File::open(&restored_path)?.read_to_end(&mut restored_bytes)?;
    let restored_hash = blake3::hash(&restored_bytes);

    assert_eq!(
        original_bytes.len(),
        restored_bytes.len(),
        "Restored size must match original"
    );
    assert_eq!(
        original_hash, restored_hash,
        "BLAKE3 checksum must match original disk exactly"
    );

    // Clean up
    let _ = std::fs::remove_file(&src_path);
    let _ = std::fs::remove_file(&relic_path);
    let _ = std::fs::remove_file(&restored_path);
    let _ = std::fs::remove_dir(&temp_dir);

    println!("[TEST PASS] Synthetic sparse disk roundtrip successfully verified.");
    Ok(())
}

// Helpers calling the internal engine methods
async fn relic_engine_capture(
    src: &std::path::Path,
    dst: &std::path::Path,
    level: i32,
    block_size_mb: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    // We can run via Command or link directly
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_relic"));
    let status = cmd
        .arg("capture")
        .arg("--source")
        .arg(src)
        .arg("--output")
        .arg(dst)
        .arg("--compression-level")
        .arg(level.to_string())
        .arg("--block-size-mb")
        .arg(block_size_mb.to_string())
        .status()?;

    assert!(status.success(), "Capture CLI command failed");
    Ok(())
}

async fn relic_engine_verify(image: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_relic"));
    let status = cmd
        .arg("verify")
        .arg("--image")
        .arg(image)
        .status()?;

    assert!(status.success(), "Verify CLI command failed");
    Ok(())
}

async fn relic_engine_restore(image: &std::path::Path, target: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_relic"));
    let status = cmd
        .arg("restore")
        .arg("--image")
        .arg(image)
        .arg("--target")
        .arg(target)
        .arg("--force")
        .status()?;

    assert!(status.success(), "Restore CLI command failed");
    Ok(())
}
