use sysinfo::Disks;

pub fn scan_devices(detailed: bool) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== [ RELIC HARDWARE DETECTOR ] ===");
    let disks = Disks::new_with_refreshed_list();

    if disks.is_empty() {
        println!("[!] No active physical block storage devices detected.");
        return Ok(());
    }

    for (index, disk) in disks.iter().enumerate() {
        let total_gb = disk.total_space() as f64 / (1024.0 * 1024.0 * 1024.0);
        let avail_gb = disk.available_space() as f64 / (1024.0 * 1024.0 * 1024.0);

        println!(
            "\nDevice #{index}: {:?} [{:?}]",
            disk.name(),
            disk.file_system()
        );
        println!("  Mount point: {:?}", disk.mount_point());
        println!("  Capacity:    {:.2} GB (Free: {:.2} GB)", total_gb, avail_gb);
        println!("  Removable:   {}", disk.is_removable());

        if detailed {
            println!("  Kind:        {:?}", disk.kind());
        }
    }
    println!("\n[+] Scan finished. Storage ready for engram operations.\n");
    Ok(())
}
