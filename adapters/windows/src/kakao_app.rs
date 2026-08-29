//! Locating the running KakaoTalk Windows app. We never launch it ourselves.

#![cfg(windows)]

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, MAX_PATH};
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// Candidate process image names. MUST be verified against a real install.
const PROCESS_NAMES: &[&str] = &["KakaoTalk.exe", "KakaoTalkEdge.exe"];

pub struct Running {
    pub pid: u32,
    pub exe_path: String,
}

pub fn running() -> Option<Running> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found: Option<u32> = None;
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let name = String::from_utf16_lossy(
                    &entry.szExeFile[..entry
                        .szExeFile
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(entry.szExeFile.len())],
                );
                if PROCESS_NAMES.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
                    found = Some(entry.th32ProcessID);
                    break;
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);

        let pid = found?;
        let exe_path = process_image_path(pid).unwrap_or_default();
        Some(Running { pid, exe_path })
    }
}

fn process_image_path(pid: u32) -> Option<String> {
    unsafe {
        let handle: HANDLE = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; MAX_PATH as usize];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(handle);
        ok.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// `FileVersion` / `ProductVersion` string from the exe's version resource.
pub fn version(exe_path: &str) -> Option<String> {
    if exe_path.is_empty() {
        return None;
    }
    unsafe {
        let wide: Vec<u16> = exe_path.encode_utf16().chain(std::iter::once(0)).collect();
        let path = windows::core::PCWSTR(wide.as_ptr());

        let size = GetFileVersionInfoSizeW(path, None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(path, 0, size, data.as_mut_ptr() as *mut _).ok()?;

        // \StringFileInfo\040904b0\ProductVersion  (Korean/Unicode is common:
        // 041204b0). Try a couple of language-codepage keys.
        for key in [
            "\\StringFileInfo\\041204b0\\ProductVersion",
            "\\StringFileInfo\\040904b0\\ProductVersion",
            "\\StringFileInfo\\041204b0\\FileVersion",
            "\\StringFileInfo\\040904b0\\FileVersion",
        ] {
            let kw: Vec<u16> = key.encode_utf16().chain(std::iter::once(0)).collect();
            let mut ptr: *mut core::ffi::c_void = std::ptr::null_mut();
            let mut len: u32 = 0;
            let ok = VerQueryValueW(
                data.as_ptr() as *const _,
                windows::core::PCWSTR(kw.as_ptr()),
                &mut ptr,
                &mut len,
            );
            if ok.as_bool() && !ptr.is_null() && len > 0 {
                let slice = std::slice::from_raw_parts(ptr as *const u16, len as usize);
                let s = String::from_utf16_lossy(slice);
                let s = s.trim_end_matches('\0').trim().to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
        None
    }
}
