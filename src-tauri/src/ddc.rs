// IOAVService carries the source address separately from these payloads.
pub fn input_read_packet() -> [u8; 4] {
    [0x82, 0x01, 0x60, 0x6e ^ 0x82 ^ 0x01 ^ 0x60]
}

pub fn input_write_packet(value: u8) -> [u8; 6] {
    [
        0x84,
        0x03,
        0x60,
        0,
        value,
        0x6e ^ 0x51 ^ 0x84 ^ 0x03 ^ 0x60 ^ value,
    ]
}

pub fn send_input(
    value: u8,
    mut write: impl FnMut(&[u8]) -> Result<(), String>,
) -> Result<(), String> {
    let packet = input_write_packet(value);
    write(&packet)?;
    // Some displays need two sends. If the first send switched away, the old
    // input may already be disconnected; readback will mark it Unconfirmed.
    let _ = write(&packet);
    Ok(())
}

pub fn parse_input_reply(reply: &[u8]) -> Result<u8, String> {
    if reply.len() < 11 || reply[0] != 0x6e || reply[1] != 0x88 || reply[2] != 0x02 {
        return Err("모니터가 올바른 DDC/CI 응답을 보내지 않았습니다.".into());
    }
    if reply[..11].iter().fold(0x50, |sum, byte| sum ^ byte) != 0 {
        return Err("모니터의 DDC/CI 응답 체크섬이 올바르지 않습니다.".into());
    }
    if reply[3] != 0 || reply[4] != 0x60 {
        return Err("이 모니터에서 현재 입력 번호 읽기를 지원하지 않습니다.".into());
    }
    let value = u16::from_be_bytes([reply[8], reply[9]]);
    u8::try_from(value).map_err(|_| format!("현재 번호 {value}가 255를 넘습니다."))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Hand-checked MCCS Get VCP reply for input 17, maximum 18.
    const HDMI_REPLY: [u8; 11] = [
        0x6e, 0x88, 0x02, 0x00, 0x60, 0x00, 0x00, 0x12, 0x00, 0x11, 0xd7,
    ];

    #[test]
    fn successful_switch_is_not_failed_when_retransmission_loses_the_old_input() {
        let mut results = [Ok(()), Err("입력 전환으로 연결 해제".into())].into_iter();
        assert_eq!(send_input(17, |_| results.next().unwrap()), Ok(()));
    }

    #[test]
    fn first_send_failure_is_reported() {
        assert_eq!(
            send_input(17, |_| Err("DDC 전송 거부".into())),
            Err("DDC 전송 거부".into())
        );
    }

    #[test]
    fn input_request_uses_vcp_60_and_ioav_checksum() {
        assert_eq!(input_read_packet(), [0x82, 0x01, 0x60, 0x8d]);
        assert_eq!(input_write_packet(17), [0x84, 0x03, 0x60, 0x00, 0x11, 0xc9]);
        // A zero checksum is still part of the packet, never truncate it.
        assert_eq!(
            input_write_packet(0xd8),
            [0x84, 0x03, 0x60, 0x00, 0xd8, 0x00]
        );
    }

    #[test]
    fn reads_input_from_a_valid_reply() {
        assert_eq!(parse_input_reply(&HDMI_REPLY), Ok(17));
    }

    #[test]
    fn rejects_empty_truncated_and_corrupt_replies() {
        assert!(parse_input_reply(&[0; 11]).is_err());
        assert!(parse_input_reply(&HDMI_REPLY[..10]).is_err());
        let mut reply = HDMI_REPLY;
        reply[8] = 1;
        assert!(parse_input_reply(&reply).is_err());
    }

    #[test]
    fn rejects_unsupported_vcp_wrong_command_and_values_above_255() {
        for (index, value) in [(3, 1), (4, 0x10), (2, 0x03), (8, 1)] {
            let mut reply = HDMI_REPLY;
            reply[index] = value;
            reply[10] = reply[..10].iter().fold(0x50, |sum, byte| sum ^ byte);
            assert!(parse_input_reply(&reply).is_err());
        }
    }
}
