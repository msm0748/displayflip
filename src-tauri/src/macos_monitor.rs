use crate::domain::{DetectedMonitor, MonitorControl};

pub struct UnverifiedMacControl;

const UNVERIFIED: &str = "이 Mac에서는 모니터 제어가 아직 확인되지 않았습니다.";

impl MonitorControl for UnverifiedMacControl {
    fn list_monitors(&mut self) -> Result<Vec<DetectedMonitor>, String> {
        Err(UNVERIFIED.into())
    }
    fn set_input(&mut self, _id: &str, _code: u8) -> Result<(), String> {
        Err(UNVERIFIED.into())
    }
    fn get_input(&mut self, _id: &str) -> Result<u8, String> {
        Err(UNVERIFIED.into())
    }
}
