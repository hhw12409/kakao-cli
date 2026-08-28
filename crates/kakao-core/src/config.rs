//! Filesystem paths and adapter-binary resolution.

use std::path::PathBuf;

use crate::error::{AppError, AppResult};

/// Where the SQLite cache lives.
///
/// - macOS:   `~/Library/Application Support/kakao-cli/kakao-cli.sqlite`
/// - Linux:   `$XDG_DATA_HOME/kakao-cli/…` (dev only)
/// - Windows: `%APPDATA%\kakao-cli\…`
pub fn db_path() -> AppResult<PathBuf> {
    if let Ok(p) = std::env::var("KAKAO_CLI_DB_PATH") {
        return Ok(PathBuf::from(p));
    }
    let dirs = directories::ProjectDirs::from("com", "hhw12409", "kakao-cli")
        .ok_or_else(|| AppError::internal("데이터 디렉토리를 찾을 수 없습니다"))?;
    let dir = dirs.data_dir();
    std::fs::create_dir_all(dir)
        .map_err(|e| AppError::internal(format!("데이터 디렉토리 생성 실패: {e}")))?;
    Ok(dir.join("kakao-cli.sqlite"))
}

/// Current OS token used in the bridge filename (`kakao-<os>-bridge`).
pub fn os_token() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unsupported"
    }
}

/// Resolve the OS adapter executable. Order (docs/adapter-contract.md §1):
///
/// 1. `KAKAO_CLI_BRIDGE_PATH`
/// 2. `<exe_dir>/../libexec/kakao-cli/kakao-<os>-bridge`
/// 3. `<exe_dir>/kakao-<os>-bridge`   (dev builds)
pub fn bridge_path() -> AppResult<PathBuf> {
    if let Ok(p) = std::env::var("KAKAO_CLI_BRIDGE_PATH") {
        return Ok(PathBuf::from(p));
    }

    let exe = std::env::current_exe()
        .map_err(|e| AppError::internal(format!("실행 경로 확인 실패: {e}")))?;
    // `current_exe` may hand back a symlink (e.g. Homebrew's bin/ shim); the
    // bridge sits next to the REAL binary in the keg, so also try the resolved
    // path.
    let name = bridge_file_name();
    let mut exe_dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = exe.parent() {
        exe_dirs.push(d.to_path_buf());
    }
    if let Ok(real) = std::fs::canonicalize(&exe) {
        if let Some(d) = real.parent() {
            if !exe_dirs.contains(&d.to_path_buf()) {
                exe_dirs.push(d.to_path_buf());
            }
        }
    }
    for dir in &exe_dirs {
        for c in [
            dir.join("..").join("libexec").join("kakao-cli").join(&name),
            dir.join(&name),
        ] {
            if c.exists() {
                return Ok(c);
            }
        }
    }
    Err(AppError::internal(format!(
        "{name} 를 찾을 수 없습니다. KAKAO_CLI_BRIDGE_PATH 로 지정하거나 재설치하세요"
    )))
}

fn bridge_file_name() -> String {
    let base = format!("kakao-{}-bridge", os_token());
    if cfg!(target_os = "windows") {
        format!("{base}.exe")
    } else {
        base
    }
}

/// True when `KAKAO_CLI_MOCK` points at a fixtures JSON file; the dispatch
/// layer then uses the in-process mock adapter instead of a subprocess.
pub fn mock_fixture_path() -> Option<PathBuf> {
    std::env::var("KAKAO_CLI_MOCK").ok().map(PathBuf::from)
}
