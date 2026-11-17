# RELIC: Hardware Bill of Materials (BOM) & Specifications

To replicate the ultra-fast recovery capabilities of the RELIC architecture, standard flash memory pen drives are insufficient due to low sustained write speeds and severe thermal throttling. The following hardware combination is recommended for optimal performance.

---

## 1. Storage Medium (The "Engram Vault")
* **Form Factor:** M.2 2280 NVMe PCIe 4.0 x4 SSD
* **Memory Type:** 3D TLC V-NAND with dedicated DRAM cache (avoids QLC write cliffs)
* **Target Capacity:** 1 TB - 4 TB
* **Verified Models:**
  * **Samsung 990 Pro / 980 Pro** (Sequential read: up to 7,450 MB/s; high write endurance)
  * **Crucial T500** (Phison E25 controller, low power draw)
  * **WD_BLACK SN850X** (Solid thermal curve under sustained loads)

---

## 2. External Bus Enclosure (The "Carrier Interface")
* **Protocol Support:** USB4 40Gbps / Thunderbolt 3 & 4
* **Bridge Controller Chipset:**
  * **Intel JHL7440** (Thunderbolt 3/4 Native Bridge)
  * **ASMedia ASM2464PD** (USB4 40Gbps / PCIe Gen4x4 bridge)
* **Chassis Construction:** CNC anodized aluminum with internal thermal pads attached directly to the NVMe controller and NAND packages.
* **Verified Enclosures:**
  * Acasis USB4 40Gbps NVMe Enclosure (TBU405 / TBU401)
  * Yottamaster Aluminum USB4 40Gbps M.2 Enclosure

---

## 3. High-Security / Forensics Alternative (Hardware Write-Blocker)
For environments where the host may be compromised by malware or ransomware:
* **Recommended Device:** **IODD 2531** or **IODD Mini**
* **Capabilities:**
  * Hardware-level read-only switch (prevents infected hosts from tampering with stored images).
  * Virtual ISO/VHD hardware emulation directly from onboard keypad and OLED display.
