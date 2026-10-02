use std::ffi::{c_char, c_void, CStr, CString};
use std::ptr::NonNull;
use std::sync::Mutex;
use std::thread::sleep;
use std::time::Duration;

use crate::ddc::{input_read_packet, parse_input_reply, send_input};
use crate::domain::{DetectedMonitor, MonitorControl};

// Keep each request/reply together across Tauri commands and shortcut handlers.
static DDC_BUS: Mutex<()> = Mutex::new(());
const GAP: Duration = Duration::from_millis(50);
const DDC_HINT: &str =
    "모니터 설정에서 DDC/CI를 켜고, 케이블이나 허브가 DDC/CI를 지원하는지 확인해주세요.";

extern "C" {
    fn df_macos_displays(status: *mut i32) -> *mut c_char;
    fn df_macos_free(value: *mut c_char);
    fn df_macos_open(location: *const c_char, chip_address: *mut u32) -> *mut c_void;
    fn df_macos_close(service: *mut c_void);
    fn df_macos_write(
        service: *mut c_void,
        chip_address: u32,
        packet: *const u8,
        length: u32,
    ) -> i32;
    fn df_macos_read(service: *mut c_void, chip_address: u32, reply: *mut u8, length: u32) -> i32;
}

#[derive(serde::Deserialize)]
struct DisplayRecord {
    id: String,
    name: String,
    location: String,
}

struct Transport {
    service: NonNull<c_void>,
    chip_address: u32,
}

// This owns a retained IOAVService; &mut MonitorControl and DDC_BUS serialize
// all use. It has no AppKit objects or thread-affine UI state.
unsafe impl Send for Transport {}

impl Transport {
    fn open(location: &str) -> Option<Self> {
        let location = CString::new(location).ok()?;
        let mut chip_address = 0x37;
        let service = unsafe { df_macos_open(location.as_ptr(), &mut chip_address) };
        NonNull::new(service).map(|service| Self {
            service,
            chip_address,
        })
    }

    fn write(&self, packet: &[u8]) -> Result<(), String> {
        sleep(GAP);
        let status = unsafe {
            df_macos_write(
                self.service.as_ptr(),
                self.chip_address,
                packet.as_ptr(),
                packet.len() as u32,
            )
        };
        if status == 0 {
            Ok(())
        } else {
            Err(format!(
                "DDC/CI 전송 실패 (0x{:08X}). {DDC_HINT}",
                status as u32
            ))
        }
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        unsafe { df_macos_close(self.service.as_ptr()) };
    }
}

struct OpenMonitor {
    detected: DetectedMonitor,
    transport: Option<Transport>,
}

pub struct MacControl {
    monitors: Result<Vec<OpenMonitor>, String>,
}

impl MacControl {
    pub fn open() -> Self {
        Self {
            monitors: enumerate().map(|records| {
                label_monitors(records)
                    .into_iter()
                    .map(|(detected, location)| OpenMonitor {
                        detected,
                        transport: Transport::open(&location),
                    })
                    .collect()
            }),
        }
    }

    fn transport(&self, id: &str) -> Result<&Transport, String> {
        let monitors = self.monitors.as_ref().map_err(Clone::clone)?;
        let monitor = monitors
            .iter()
            .find(|monitor| monitor.detected.id == id)
            .ok_or_else(|| {
                format!("모니터를 찾을 수 없습니다: {id}. 모니터 목록을 새로고침해주세요.")
            })?;
        monitor.transport.as_ref().ok_or_else(|| {
            if cfg!(target_arch = "aarch64") {
                format!("이 연결에서는 모니터의 DDC/CI 제어 경로를 찾지 못했습니다. {DDC_HINT}")
            } else {
                "현재 macOS 입력 전환은 Apple Silicon Mac에서 지원합니다.".into()
            }
        })
    }
}

impl MonitorControl for MacControl {
    fn list_monitors(&mut self) -> Result<Vec<DetectedMonitor>, String> {
        self.monitors
            .as_ref()
            .map(|monitors| {
                monitors
                    .iter()
                    .map(|monitor| monitor.detected.clone())
                    .collect()
            })
            .map_err(Clone::clone)
    }

    fn set_input(&mut self, id: &str, code: u8) -> Result<(), String> {
        let _bus = DDC_BUS
            .lock()
            .map_err(|_| "모니터 통신 잠금을 얻지 못했습니다.".to_string())?;
        let transport = self.transport(id)?;
        send_input(code, |packet| transport.write(packet))?;
        sleep(Duration::from_millis(150));
        Ok(())
    }

    fn get_input(&mut self, id: &str) -> Result<u8, String> {
        let _bus = DDC_BUS
            .lock()
            .map_err(|_| "모니터 통신 잠금을 얻지 못했습니다.".to_string())?;
        let transport = self.transport(id)?;
        let mut last_error = String::new();
        for _ in 0..3 {
            if let Err(error) = transport.write(&input_read_packet()) {
                last_error = error;
                continue;
            }
            sleep(GAP);
            let mut reply = [0u8; 12];
            let status = unsafe {
                df_macos_read(
                    transport.service.as_ptr(),
                    transport.chip_address,
                    reply.as_mut_ptr(),
                    reply.len() as u32,
                )
            };
            if status != 0 {
                last_error = format!("DDC/CI 읽기 실패 (0x{:08X})", status as u32);
                continue;
            }
            match parse_input_reply(&reply) {
                Ok(value) => return Ok(value),
                Err(error) => last_error = error,
            }
        }
        Err(format!(
            "현재 입력 번호를 읽지 못했습니다. {last_error} {DDC_HINT}"
        ))
    }
}

fn enumerate() -> Result<Vec<DisplayRecord>, String> {
    let mut status = 0;
    let json = unsafe { df_macos_displays(&mut status) };
    if json.is_null() {
        return Err(format!(
            "macOS 모니터 목록을 읽지 못했습니다. 코드 {status}"
        ));
    }
    // Copy/deserialize while the allocation is live, then free it even on error.
    let records = serde_json::from_slice(unsafe { CStr::from_ptr(json) }.to_bytes());
    unsafe { df_macos_free(json) };
    records.map_err(|error| format!("macOS 모니터 정보를 해석하지 못했습니다: {error}"))
}

fn label_monitors(mut records: Vec<DisplayRecord>) -> Vec<(DetectedMonitor, String)> {
    records.sort_by(|left, right| left.id.cmp(&right.id));
    let mut counts = std::collections::HashMap::new();
    for record in &records {
        *counts.entry(record.name.clone()).or_insert(0usize) += 1;
    }
    let mut positions = std::collections::HashMap::new();
    records
        .into_iter()
        .map(|record| {
            let position = positions.entry(record.name.clone()).or_insert(0usize);
            *position += 1;
            let name = if counts[&record.name] > 1 {
                format!("{} ({position})", record.name)
            } else {
                record.name
            };
            (
                DetectedMonitor {
                    id: record.id,
                    name,
                },
                record.location,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_model_and_zero_serial_monitors_keep_distinct_ids() {
        let records = vec![
            DisplayRecord {
                id: "uuid-b".into(),
                name: "Q27C15".into(),
                location: "port-b".into(),
            },
            DisplayRecord {
                id: "uuid-a".into(),
                name: "Q27C15".into(),
                location: "port-a".into(),
            },
        ];
        let labeled = label_monitors(records);
        assert_eq!(
            labeled[0],
            (
                DetectedMonitor {
                    id: "uuid-a".into(),
                    name: "Q27C15 (1)".into()
                },
                "port-a".into()
            )
        );
        assert_eq!(
            labeled[1],
            (
                DetectedMonitor {
                    id: "uuid-b".into(),
                    name: "Q27C15 (2)".into()
                },
                "port-b".into()
            )
        );
    }

    #[test]
    fn displays_without_ddc_are_still_listed() {
        let mut control = MacControl {
            monitors: Ok(vec![OpenMonitor {
                detected: DetectedMonitor {
                    id: "uuid-a".into(),
                    name: "Q27C15".into(),
                },
                transport: None,
            }]),
        };
        assert_eq!(control.list_monitors().unwrap().len(), 1);
        assert!(control.get_input("uuid-a").is_err());
        assert!(control.set_input("uuid-a", 17).is_err());
        assert!(control
            .set_input("missing", 17)
            .unwrap_err()
            .contains("모니터를 찾을 수 없습니다"));
    }

    #[test]
    #[ignore = "requires external DDC/CI monitors and access to WindowServer"]
    fn probe_connected_monitors_read_only() {
        let mut control = MacControl::open();
        let monitors = control.list_monitors().unwrap();
        assert!(!monitors.is_empty(), "외부 모니터가 연결돼 있어야 합니다.");
        for monitor in monitors {
            let input = control.get_input(&monitor.id);
            println!("{} [{}]: {input:?}", monitor.name, monitor.id);
            assert!(input.is_ok(), "{input:?}");
        }
    }
}
