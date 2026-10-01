pub fn edid_stable_id(edid: &[u8]) -> Option<String> {
    if edid.len() < 16 || edid[..8] != [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00] {
        return None;
    }
    let word = u16::from_be_bytes([edid[8], edid[9]]);
    let letters = [
        letter((word >> 10) & 0x1F)?,
        letter((word >> 5) & 0x1F)?,
        letter(word & 0x1F)?,
    ];
    let product = u16::from_le_bytes([edid[10], edid[11]]);
    let serial = u32::from_le_bytes([edid[12], edid[13], edid[14], edid[15]]);
    if serial == 0 {
        return None;
    }
    Some(format!("{}{product:04X}-{serial}", String::from_utf8_lossy(&letters)))
}

fn letter(value: u16) -> Option<u8> {
    if (1..=26).contains(&value) {
        Some(b'A' + (value as u8 - 1))
    } else {
        None
    }
}

pub fn display_interface_parts(name: &str) -> Option<(String, String)> {
    let rest = name.strip_prefix(r"\\?\DISPLAY#")?;
    let mut parts = rest.split('#');
    let hwid = parts.next()?;
    let instance = parts.next()?;
    let class_guid = parts.next()?;
    if parts.next().is_some()
        || hwid.is_empty()
        || instance.is_empty()
        || hwid.contains('\\')
        || instance.contains('\\')
        || !is_class_guid(class_guid)
    {
        return None;
    }
    Some((hwid.to_string(), instance.to_string()))
}

fn is_class_guid(value: &str) -> bool {
    let Some(inner) = value.strip_prefix('{').and_then(|rest| rest.strip_suffix('}')) else {
        return false;
    };
    !inner.is_empty() && !inner.contains('{') && !inner.contains('}')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(serial: u32) -> Vec<u8> {
        let mut edid = vec![0u8; 128];
        edid[..8].copy_from_slice(&[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00]);
        let word: u16 = (1 << 10) | (2 << 5) | 3;
        edid[8..10].copy_from_slice(&word.to_be_bytes());
        edid[10..12].copy_from_slice(&0x1234u16.to_le_bytes());
        edid[12..16].copy_from_slice(&serial.to_le_bytes());
        edid
    }

    #[test]
    fn edid_id_uses_manufacturer_product_and_serial() {
        assert_eq!(edid_stable_id(&sample(42)).as_deref(), Some("ABC1234-42"));
    }

    #[test]
    fn zero_serial_has_no_edid_id() {
        assert_eq!(edid_stable_id(&sample(0)), None);
    }

    #[test]
    fn short_edid_has_no_id() {
        assert_eq!(edid_stable_id(&[0, 1, 2]), None);
    }

    #[test]
    fn display_interface_parts_reads_hwid_and_enum_instance() {
        let fixtures = [
            (
                r"\\?\DISPLAY#GSM5B09#5&2a3b4c&0&UID4352#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}",
                "GSM5B09",
                "5&2a3b4c&0&UID4352",
            ),
            (
                r"\\?\DISPLAY#DELA007#5&2f8c3d4&0&UID4352#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}",
                "DELA007",
                "5&2f8c3d4&0&UID4352",
            ),
        ];
        for (name, hwid, instance) in fixtures {
            assert_eq!(display_interface_parts(name), Some((hwid.to_string(), instance.to_string())));
        }
    }

    #[test]
    fn display_interface_parts_rejects_driver_index_device_id() {
        assert_eq!(
            display_interface_parts(r"MONITOR\GSM5B09\{4d36e96e-e325-11ce-bfc1-08002be10318}\0001"),
            None
        );
    }

    #[test]
    fn display_interface_parts_rejects_incomplete_name() {
        assert_eq!(display_interface_parts(r"\\?\DISPLAY#GSM5B09#5&2a3b4c&0&UID4352"), None);
        assert_eq!(display_interface_parts(r"\\?\DISPLAY#GSM5B09"), None);
        assert_eq!(display_interface_parts(""), None);
    }
}
