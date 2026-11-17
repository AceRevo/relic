# RELIC: System Architecture & Execution Lifecycle

```
+-------------------------------------------------------------------------------+
|                             PHASE 1: HOST FAILURE                             |
|  - Operating system corrupted, BSOD loop, ransomware, or bootloader collapse   |
|  - Host enters "Flatline" condition                                           |
+-------------------------------------------------------------------------------+
                                        |
                                        v
+-------------------------------------------------------------------------------+
|                       PHASE 2: RELIC INSERTION & UEFI BOOT                    |
|  - Fast NVMe enclosure connected over USB4 / Thunderbolt (up to 40 Gbps)      |
|  - UEFI bootloader executes minimal Alpine/Busybox Initramfs into RAM (<2s)    |
|  - Host disk is unmounted; memory isolation verified                          |
+-------------------------------------------------------------------------------+
                                        |
                                        v
+-------------------------------------------------------------------------------+
|                       PHASE 3: TAKEOVER & REANIMATION                         |
|  [Mode A: Instant Overlay]                 [Mode B: Direct Block Overwrite]   |
|  - Host boots into memory overlay (RAM)    - Zstandard multi-threaded stream  |
|  - User accesses files immediately         - Direct DMA write to NVMe         |
|  - Background repair runs in parallel      - Sustained speed: 2.0 - 3.2 GB/s  |
+-------------------------------------------------------------------------------+
                                        |
                                        v
+-------------------------------------------------------------------------------+
|                          PHASE 4: INTEGRITY & REBOOT                          |
|  - BLAKE3 block validation confirms zero corruption                           |
|  - Kernel syncs buffer cache to disk sectors                                  |
|  - Host reboot triggered: machine returns to clean, operational state         |
+-------------------------------------------------------------------------------+
```

---

## 1. Minimalistic Initramfs Architecture
The host container is compiled via Buildroot / Alpine Linux:
* **Kernel footprint:** Sub-20 MB Linux kernel with non-essential modules stripped.
* **Driver subset:** NVMe over PCIe, AHCI SATA, xHCI USB 3/4, KMS graphic framebuffers.
* **PID 1 Execution:** The compiled Rust binary (`relic`) is executed directly as `/init` without spinning up unnecessary system daemons (systemd, journald, dbus).

## 2. Block-Level Streaming Engine
Unlike file-by-file synchronization tools (such as rsync) which incur enormous metadata and inode traversal penalties, RELIC operates at the **raw block layer**:
1. Reads disk geometry and partition offsets (GPT/MBR headers).
2. Utilizes sparse block detection: contiguous null blocks (`0x00`) are omitted from compression streams.
3. Streams active sectors through parallel Zstandard compressors with custom frame headers.
4. Restores directly to the raw block device node (`/dev/nvmeXn1`).

## 3. High-Throughput Verification
Each 64 MB chunk is signed with a BLAKE3 tree hash. BLAKE3 is utilized due to its hardware AVX-512 / NEON acceleration, allowing verification throughput exceeding 6 GB/s per core, ensuring disk writing is never bottlenecked by cryptographic checks.
