//! 🟢 Hardware Hex-to-String Lookup Dictionary
//! Converts raw silicon hex codes (SMBIOS, PCI) into human-readable strings.
//! Zero hardcoding logic.

pub struct HardwareLookup;

impl HardwareLookup {
    /// Maps SMBIOS Memory Type IDs to Strings
    pub fn get_ram_type(type_id: u16) -> String {
        match type_id {
            20 => "DDR".into(),
            21 => "DDR2".into(),
            24 => "DDR3".into(),
            26 => "DDR4".into(),
            29 => "LPDDR4".into(),
            30 => "LPDDR4X".into(),
            34 => "DDR5".into(),
            35 => "LPDDR5".into(),
            36 => "LPDDR5X".into(),
            _ => format!("RAW_TYPE_0x{:X}", type_id),
        }
    }

    /// Maps PCI Vendor IDs to Strings
    pub fn get_gpu_vendor(vendor_id: u16) -> String {
        match vendor_id {
            0x10DE => "NVIDIA_CORP".into(),
            0x1002 => "AMD".into(),
            0x8086 => "INTEL".into(),
            0x106B => "APPLE".into(),
            _ => format!("RAW_VENDOR_0x{:X}", vendor_id),
        }
    }
}
