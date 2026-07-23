use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

// timeout.exe polls console input to detect Ctrl+C even with /NOBREAK, so it
// refuses to run ("ERROR: Input redirection is not supported, exiting the
// process immediately.") when its stdin isn't a real console -- which is the
// case whenever `cargo test` itself runs without one (headless CI runners,
// scheduled tasks, this repo's own agent-driven test runs). CREATE_NO_WINDOW
// gives the child a real (but hidden) console, sidestepping that without
// popping a visible console window on screen the way CREATE_NEW_CONSOLE would.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[path = "../src/inject.rs"]
mod inject;

/// Гарантирует, что порождённый процесс будет убит, даже если assert внутри
/// теста запаникует до того, как дойдёт до обычной очистки в конце функции.
struct KillOnDrop(Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn marker_dll_path() -> PathBuf {
    // Воркспейс шарит один target/ каталог между членами workspace.
    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("target")
        .join(profile)
        .join("marker_dll_fixture.dll")
}

fn marker_log_path() -> PathBuf {
    std::env::temp_dir().join("dark_inject_marker.log")
}

#[test]
fn injects_marker_dll_into_real_process_and_confirms_load() {
    let dll_path = marker_dll_path();
    assert!(
        dll_path.exists(),
        "marker_dll_fixture.dll not built at {:?}; run `cargo build -p marker_dll_fixture` first",
        dll_path
    );

    let _ = std::fs::remove_file(marker_log_path());

    // timeout.exe — реальный процесс Windows, не требует прав администратора,
    // безопасно убить в конце теста.
    let child = Command::new("timeout")
        .args(["/T", "20", "/NOBREAK"])
        .stdout(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .expect("failed to spawn timeout.exe");
    let mut guard = KillOnDrop(child);

    let pid = guard.0.id();

    let result = inject::inject_dll(pid, &dll_path);
    assert!(result.is_ok(), "inject_dll failed: {:?}", result);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut found = false;
    while Instant::now() < deadline {
        if let Ok(contents) = std::fs::read_to_string(marker_log_path()) {
            if contents.contains(&format!("loaded pid={}", pid)) {
                found = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let _ = guard.0.kill();
    let _ = guard.0.wait();
    let _ = std::fs::remove_file(marker_log_path());

    assert!(found, "marker DLL did not confirm load into pid {}", pid);
}
