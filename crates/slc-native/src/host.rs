//! Host linking for the x86-64 ELF. `slc` and the native tests share it.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use slc_abi::{
    SLC_RT_START, START_SLC_LABELS, START_SLC_MAPS, START_SLC_POOL_PTRS, START_SLC_POOL_SCALARS,
    START_SLC_SAFEPOINTS, START_SLC_TEXT, STOP_SLC_LABELS, STOP_SLC_MAPS, STOP_SLC_POOL_PTRS,
    STOP_SLC_POOL_SCALARS, STOP_SLC_SAFEPOINTS, STOP_SLC_TEXT,
};

/// x86-64 Linux is the host whose `cc` accepts `-pie` and `--gc-sections`
/// and whose kernel runs the ELF this compiler emits.
pub fn host_links_elf() -> bool {
    cfg!(all(target_arch = "x86_64", target_os = "linux"))
}

/// `libslc_rt.a` from the `release-abort` profile. Built at most once
/// per process; a failed build is not cached.
pub fn runtime_archive() -> Result<PathBuf, String> {
    static CACHE: OnceLock<PathBuf> = OnceLock::new();
    if let Some(path) = CACHE.get() {
        return Ok(path.clone());
    }
    let path = build_runtime_archive()?;
    Ok(CACHE.get_or_init(|| path).clone())
}

/// Libraries `rustc --print native-static-libs` reports for this host,
/// including `-lm`. A failed query is not cached.
pub fn native_static_libs() -> Result<Vec<String>, String> {
    static CACHE: OnceLock<Vec<String>> = OnceLock::new();
    if let Some(libs) = CACHE.get() {
        return Ok(libs.clone());
    }
    let libs = query_native_libs()?;
    Ok(CACHE.get_or_init(|| libs).clone())
}

/// Link `object` and the C `driver` into `exe` with the runtime.
/// Scratch files are the caller's.
pub fn link_pie(exe: &Path, driver: &Path, object: &Path) -> Result<(), String> {
    if !host_links_elf() {
        return Err("cannot link an x86-64 ELF on this host".into());
    }
    let mut cmd = Command::new("cc");
    cmd.args(["-fPIE", "-pie", "-Wl,--gc-sections", "-o"])
        .arg(exe)
        .arg(driver)
        .arg(object)
        .arg(runtime_archive()?);
    cmd.args(native_static_libs()?);
    let linked = cmd.output().map_err(|error| format!("cannot run cc: {error}"))?;
    if !linked.status.success() {
        return Err(format!(
            "{}{}",
            String::from_utf8_lossy(&linked.stderr),
            String::from_utf8_lossy(&linked.stdout)
        ));
    }
    Ok(())
}

/// `extern` declarations of `slc_rt_start` and the GNU section bounds,
/// in the order `slc_rt_start` reads them.
pub fn entry_externs() -> String {
    format!(
        "\
extern uint64_t {SLC_RT_START}(
    uint64_t fuel,
    const void *safepoints_start, const void *safepoints_stop,
    const void *maps_start, const void *maps_stop,
    const void *text_start, const void *text_stop,
    const void *scalars_start, const void *scalars_stop,
    const void *ptrs_start, const void *ptrs_stop,
    const void *labels_start, const void *labels_stop);
extern char {START_SLC_SAFEPOINTS}, {STOP_SLC_SAFEPOINTS};
extern char {START_SLC_MAPS}, {STOP_SLC_MAPS};
extern char {START_SLC_TEXT}, {STOP_SLC_TEXT};
extern char {START_SLC_POOL_SCALARS}, {STOP_SLC_POOL_SCALARS};
extern char {START_SLC_POOL_PTRS}, {STOP_SLC_POOL_PTRS};
extern char {START_SLC_LABELS}, {STOP_SLC_LABELS};
"
    )
}

/// The `slc_rt_start` call. `fuel` is a C expression.
pub fn entry_call(fuel: &str) -> String {
    format!(
        "{SLC_RT_START}(\n        {fuel},\n        \
         &{START_SLC_SAFEPOINTS}, &{STOP_SLC_SAFEPOINTS},\n        \
         &{START_SLC_MAPS}, &{STOP_SLC_MAPS},\n        \
         &{START_SLC_TEXT}, &{STOP_SLC_TEXT},\n        \
         &{START_SLC_POOL_SCALARS}, &{STOP_SLC_POOL_SCALARS},\n        \
         &{START_SLC_POOL_PTRS}, &{STOP_SLC_POOL_PTRS},\n        \
         &{START_SLC_LABELS}, &{STOP_SLC_LABELS})"
    )
}

fn workspace_root() -> Result<PathBuf, String> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|crates| crates.parent())
        .map(Path::to_path_buf)
        .ok_or_else(|| "cannot find the workspace root".into())
}

fn build_runtime_archive() -> Result<PathBuf, String> {
    let workspace = workspace_root()?;
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map_or_else(|| workspace.join("target"), PathBuf::from);
    let output = Command::new(env!("CARGO"))
        .current_dir(&workspace)
        .args(["build", "-p", "slc-rt", "--profile", "release-abort", "--offline", "--target-dir"])
        .arg(&target)
        .output()
        .map_err(|error| format!("cannot build the runtime: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{}{}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        ));
    }
    Ok(target.join("release-abort/libslc_rt.a"))
}

/// rustc names the stdin crate `librust_out` and writes that archive into
/// `--out-dir`. A private directory keeps it out of the source tree.
fn query_native_libs() -> Result<Vec<String>, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let out_dir = std::env::temp_dir().join(format!(
        "slc-native-libs-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&out_dir).map_err(|error| format!("cannot run rustc: {error}"))?;
    let guard = DirGuard(out_dir.clone());
    let mut child = Command::new("rustc")
        .current_dir(&out_dir)
        .args([
            "--print",
            "native-static-libs",
            "--crate-type",
            "staticlib",
            "--edition",
            "2024",
            "--out-dir",
        ])
        .arg(&out_dir)
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot run rustc: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        if let Err(error) = stdin.write_all(b"pub fn slc() {}\n") {
            drop(stdin);
            let _ = child.wait();
            return Err(format!("cannot run rustc: {error}"));
        }
    }
    let output = child.wait_with_output().map_err(|error| format!("cannot run rustc: {error}"))?;
    drop(guard);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let line = text
        .lines()
        .find_map(|line| {
            line.split("native-static-libs:").nth(1).or_else(|| {
                if line.split_whitespace().any(|word| word.starts_with("-l")) {
                    Some(line)
                } else {
                    None
                }
            })
        })
        .unwrap_or("");
    let libs: Vec<String> = line.split_whitespace().map(str::to_string).collect();
    if libs.is_empty() {
        return Err(format!(
            "cannot read host libraries: rustc --print native-static-libs reported no libraries\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(libs)
}

struct DirGuard(PathBuf);

impl Drop for DirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_text_names_the_section_bounds() {
        let text = entry_externs();
        assert!(text.contains(slc_abi::SLC_RT_START));
        assert!(text.contains(slc_abi::START_SLC_TEXT));
        assert!(text.contains(slc_abi::STOP_SLC_LABELS));
        let call = entry_call("fuel");
        assert!(call.starts_with("slc_rt_start("));
        assert!(call.contains("&__start_slc_text"));
        assert!(call.contains("fuel"));
    }
}
