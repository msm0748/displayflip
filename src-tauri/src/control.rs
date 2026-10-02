use crate::domain::MonitorControl;

pub fn open_control() -> Box<dyn MonitorControl + Send> {
    #[cfg(windows)]
    {
        Box::new(crate::windows_monitor::WindowsControl::open())
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(crate::macos_monitor::MacControl::open())
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        Box::new(UnsupportedControl)
    }
}

struct UnsupportedControl;

impl MonitorControl for UnsupportedControl {
    fn list_monitors(&mut self) -> Result<Vec<crate::domain::DetectedMonitor>, String> {
        Err("이 운영체제에서는 모니터 제어를 지원하지 않습니다.".into())
    }
    fn set_input(&mut self, _id: &str, _code: u8) -> Result<(), String> {
        Err("이 운영체제에서는 모니터 제어를 지원하지 않습니다.".into())
    }
    fn get_input(&mut self, _id: &str) -> Result<u8, String> {
        Err("이 운영체제에서는 모니터 제어를 지원하지 않습니다.".into())
    }
}
