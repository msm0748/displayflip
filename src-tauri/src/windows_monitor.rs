use std::collections::HashMap;

use windows::Win32::Devices::Display::{
    DestroyPhysicalMonitors, GetNumberOfPhysicalMonitorsFromHMONITOR,
    GetPhysicalMonitorsFromHMONITOR, GetVCPFeatureAndVCPFeatureReply, SetVCPFeature,
    PHYSICAL_MONITOR,
};
use windows::Win32::Foundation::{ERROR_SUCCESS, HANDLE, LPARAM};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, DISPLAY_DEVICEW, HMONITOR,
    MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ,
};
use windows::core::{BOOL, PCWSTR};

use crate::domain::{DetectedMonitor, MonitorControl};
use crate::identity::edid_stable_id;

const INPUT_SELECT: u8 = 0x60;

struct OpenMonitor {
    handle: HANDLE,
    name: String,
}

pub struct WindowsControl {
    monitors: HashMap<String, OpenMonitor>,
}

// windows 0.62 HANDLE is `*mut c_void` and is not Send.
// This value owns the physical-monitor handles and uses them only through `&mut self`.
unsafe impl Send for WindowsControl {}

impl WindowsControl {
    pub fn open() -> Self {
        let mut monitors = HashMap::new();
        for found in enumerate_handles() {
            let id = found.device_id.clone();
            monitors.insert(id, OpenMonitor { handle: found.handle, name: found.name });
        }
        Self { monitors }
    }
}

impl Drop for WindowsControl {
    fn drop(&mut self) {
        let handles: Vec<PHYSICAL_MONITOR> = self
            .monitors
            .values()
            .map(|monitor| PHYSICAL_MONITOR {
                hPhysicalMonitor: monitor.handle,
                szPhysicalMonitorDescription: [0; 128],
            })
            .collect();
        if !handles.is_empty() {
            unsafe { let _ = DestroyPhysicalMonitors(&handles); }
        }
    }
}

impl MonitorControl for WindowsControl {
    fn list_monitors(&mut self) -> Result<Vec<DetectedMonitor>, String> {
        Ok(self
            .monitors
            .iter()
            .map(|(id, monitor)| DetectedMonitor { id: id.clone(), name: monitor.name.clone() })
            .collect())
    }

    fn set_input(&mut self, id: &str, code: u8) -> Result<(), String> {
        let handle = self.monitors.get(id).ok_or_else(|| format!("모니터를 찾을 수 없습니다: {id}"))?.handle;
        let status = unsafe { SetVCPFeature(handle, INPUT_SELECT, u32::from(code)) };
        if status == 0 {
            Err(format!("입력 번호 {code} 전송이 거부되었습니다. 코드 {status}"))
        } else {
            Ok(())
        }
    }

    fn get_input(&mut self, id: &str) -> Result<u8, String> {
        let handle = self.monitors.get(id).ok_or_else(|| format!("모니터를 찾을 수 없습니다: {id}"))?.handle;
        let mut current = 0u32;
        let status = unsafe {
            GetVCPFeatureAndVCPFeatureReply(
                handle,
                INPUT_SELECT,
                None,
                std::ptr::from_mut(&mut current),
                None,
            )
        };
        if status == 0 {
            Err(format!("현재 번호를 읽지 못했습니다. 코드 {status}"))
        } else {
            u8::try_from(current).map_err(|_| format!("현재 번호 {current}가 255를 넘습니다."))
        }
    }
}

struct Found {
    handle: HANDLE,
    name: String,
    device_id: String,
}

fn enumerate_handles() -> Vec<Found> {
    let mut raw = Vec::new();
    unsafe {
        let pointer = &mut raw as *mut Vec<HMONITOR>;
        let _ = EnumDisplayMonitors(None, None, Some(push_monitor), LPARAM(pointer as isize));
    }
    raw.into_iter().filter_map(open_physical).collect()
}

unsafe extern "system" fn push_monitor(monitor: HMONITOR, _: windows::Win32::Graphics::Gdi::HDC, _: *mut windows::Win32::Foundation::RECT, data: LPARAM) -> BOOL {
    let list = &mut *(data.0 as *mut Vec<HMONITOR>);
    list.push(monitor);
    BOOL(1)
}

fn open_physical(monitor: HMONITOR) -> Option<Found> {
    let mut count = 0u32;
    unsafe { GetNumberOfPhysicalMonitorsFromHMONITOR(monitor, std::ptr::from_mut(&mut count)).ok()? };
    if count == 0 {
        return None;
    }
    let mut physical = vec![PHYSICAL_MONITOR::default(); count as usize];
    unsafe { GetPhysicalMonitorsFromHMONITOR(monitor, &mut physical).ok()? };
    let first = physical[0];
    if physical.len() > 1 {
        unsafe { let _ = DestroyPhysicalMonitors(&physical[1..]); }
    }
    let device_name = monitor_device_name(monitor)?;
    let device_id = display_device_id(&device_name).unwrap_or(device_name);
    let stable = read_edid(&device_id).and_then(|bytes| edid_stable_id(&bytes)).unwrap_or(device_id);
    let description = first.szPhysicalMonitorDescription;
    Some(Found {
        handle: first.hPhysicalMonitor,
        name: utf16_to_string(&description),
        device_id: stable,
    })
}

fn monitor_device_name(monitor: HMONITOR) -> Option<String> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    let ok = unsafe { GetMonitorInfoW(monitor, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO) };
    if ok.as_bool() { Some(utf16_to_string(&info.szDevice)) } else { None }
}

fn display_device_id(device_name: &str) -> Option<String> {
    let mut device = DISPLAY_DEVICEW::default();
    device.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
    let name = wide(device_name);
    let ok = unsafe { EnumDisplayDevicesW(PCWSTR(name.as_ptr()), 0, &mut device, 0) };
    if ok.as_bool() { Some(utf16_to_string(&device.DeviceID)) } else { None }
}

fn read_edid(device_id: &str) -> Option<Vec<u8>> {
    let (hwid, instance) = device_id.split('\\').nth(1).zip(device_id.split('\\').nth(3))?;
    let path = format!("SYSTEM\\CurrentControlSet\\Enum\\DISPLAY\\{hwid}\\{instance}\\Device Parameters");
    let wide_path = wide(&path);
    let mut key = HKEY::default();
    let opened = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(wide_path.as_ptr()),
            Some(0),
            KEY_READ,
            std::ptr::from_mut(&mut key),
        )
    };
    if opened != ERROR_SUCCESS {
        return None;
    }
    let value = wide("EDID");
    let mut size = 0u32;
    let _ = unsafe {
        RegQueryValueExW(
            key,
            PCWSTR(value.as_ptr()),
            None,
            None,
            None,
            Some(std::ptr::from_mut(&mut size)),
        )
    };
    let mut buffer = vec![0u8; size as usize];
    let status = unsafe {
        RegQueryValueExW(
            key,
            PCWSTR(value.as_ptr()),
            None,
            None,
            Some(buffer.as_mut_ptr()),
            Some(std::ptr::from_mut(&mut size)),
        )
    };
    unsafe { let _ = RegCloseKey(key); }
    if status == ERROR_SUCCESS { Some(buffer) } else { None }
}

fn utf16_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|unit| *unit == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end]).trim().to_string()
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
