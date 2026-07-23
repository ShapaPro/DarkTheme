use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

// timeout.exe polls console input to detect Ctrl+C even with /NOBREAK, so it
// refuses to run ("ERROR: Input redirection is not supported, exiting the
// process immediately.") when its stdin isn't a real console -- which is the
// case whenever `cargo test` itself runs without one (headless CI runners,
// scheduled tasks, this repo's own agent-driven test runs). Giving the child
// its own console with CREATE_NEW_CONSOLE sidesteps that regardless of what
// console (if any) the test runner has.
const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

#[path = "../src/inject.rs"]
mod inject;

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
    let mut child = Command::new("timeout")
        .args(["/T", "20", "/NOBREAK"])
        .stdout(std::process::Stdio::null())
        .creation_flags(CREATE_NEW_CONSOLE)
        .spawn()
        .expect("failed to spawn timeout.exe");

    let pid = child.id();

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

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(marker_log_path());

    assert!(found, "marker DLL did not confirm load into pid {}", pid);
}
