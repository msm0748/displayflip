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
}
