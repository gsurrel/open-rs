use std::{
    ffi::{OsStr, OsString},
    process::Command,
};

use std::os::windows::process::CommandExt;

const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Returns true if running inside Wine (runtime detection, not compile time).
fn is_running_under_wine() -> bool {
    std::env::var_os("WINEPREFIX").is_some()
        || std::env::var_os("WINELOADER").is_some()
        || std::env::var_os("WINEDEBUG").is_some()
}

fn winebrowser_command<T: AsRef<OsStr>>(path: T) -> Command {
    let mut cmd = Command::new("winebrowser");
    cmd.arg(path.as_ref());
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// Returns a sequence of commands to open a path, using winebrowser under Wine, else normal Windows fallback.
pub fn commands<T: AsRef<OsStr>>(path: T) -> Vec<Command> {
    let mut cmds = Vec::new();
    #[cfg(debug_assertions)]
    if is_running_under_wine() {
        eprintln!("[open-rs] Wine detected: prepending winebrowser launcher");
    }
    if is_running_under_wine() {
        cmds.push(winebrowser_command(&path));
    }
    let mut cmd = Command::new("cmd");
    cmd.arg("/c")
        .arg("start")
        .raw_arg("\"\"")
        .raw_arg(wrap_in_quotes(path))
        .creation_flags(CREATE_NO_WINDOW);
    cmds.push(cmd);
    cmds
}

/// Returns a command to open a path with a specific app, using winebrowser under Wine, else normal Windows fallback.
pub fn with_command<T: AsRef<OsStr>>(path: T, app: impl Into<String>) -> Command {
    if is_running_under_wine() {
        let mut cmd = Command::new("winebrowser");
        cmd.arg(app.into());
        cmd.arg(path.as_ref());
        cmd.creation_flags(CREATE_NO_WINDOW);
        #[cfg(debug_assertions)]
        eprintln!("[open-rs] Wine detected: with_command delegates to winebrowser");
        return cmd;
    }
    let mut cmd = Command::new("cmd");
    cmd.arg("/c")
        .arg("start")
        .raw_arg("\"\"")
        .raw_arg(wrap_in_quotes(app.into()))
        .raw_arg(wrap_in_quotes(path))
        .creation_flags(CREATE_NO_WINDOW);
    cmd
}

fn wrap_in_quotes<T: AsRef<OsStr>>(path: T) -> OsString {
    let mut result = OsString::from("\"");
    result.push(path);
    result.push("\"");
    result
}

#[cfg(feature = "shellexecute-on-windows")]
pub fn that_detached<T: AsRef<OsStr>>(path: T) -> std::io::Result<()> {
    let path = path.as_ref();
    let is_dir = std::fs::metadata(path).map(|f| f.is_dir()).unwrap_or(false);

    if is_dir && shell_open_folder(path).is_ok() {
        return Ok(());
    };

    let path = wide(path);

    let (verb, class) = if is_dir {
        (ffi::EXPLORE, ffi::FOLDER)
    } else {
        (std::ptr::null(), std::ptr::null())
    };

    let mut info = ffi::SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<ffi::SHELLEXECUTEINFOW>() as _,
        nShow: ffi::SW_SHOWNORMAL,
        lpVerb: verb,
        lpClass: class,
        lpFile: path.as_ptr(),
        ..unsafe { std::mem::zeroed() }
    };

    unsafe { ShellExecuteExW(&mut info) }
}

#[cfg(feature = "shellexecute-on-windows")]
fn shell_open_folder(path: &OsStr) -> Result<(), std::io::Error> {
    let path = std::path::absolute(path)?;
    let path = wide(dunce::simplified(&path));
    unsafe { ffi::CoInitialize(std::ptr::null()) };
    let folder = unsafe { ffi::ILCreateFromPathW(path.as_ptr()) };
    if folder.is_null() {
        return Err(std::io::Error::last_os_error());
    }
    let result = unsafe { SHOpenFolderAndSelectItems(folder, Some(&[folder]), 0) };
    unsafe { ffi::ILFree(folder) };
    result
}

#[cfg(feature = "shellexecute-on-windows")]
pub fn with_detached<T: AsRef<OsStr>>(path: T, app: impl Into<String>) -> std::io::Result<()> {
    let app = wide(app.into());
    let path = wide(path);

    let mut info = ffi::SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<ffi::SHELLEXECUTEINFOW>() as _,
        nShow: ffi::SW_SHOWNORMAL,
        lpFile: app.as_ptr(),
        lpParameters: path.as_ptr(),
        ..unsafe { std::mem::zeroed() }
    };

    unsafe { ShellExecuteExW(&mut info) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn clear_wine_env() {
        env::remove_var("WINEPREFIX");
        env::remove_var("WINELOADER");
        env::remove_var("WINEDEBUG");
    }

    #[test]
    fn wine_detection_by_wineprefix() {
        clear_wine_env();
        env::set_var("WINEPREFIX", "/fake");
        assert!(is_running_under_wine());
        env::remove_var("WINEPREFIX");
    }

    #[test]
    fn wine_detection_by_wineloader() {
        clear_wine_env();
        env::set_var("WINELOADER", "1");
        assert!(is_running_under_wine());
        env::remove_var("WINELOADER");
    }

    #[test]
    fn wine_detection_by_winedebug() {
        clear_wine_env();
        env::set_var("WINEDEBUG", "fixme-all");
        assert!(is_running_under_wine());
        env::remove_var("WINEDEBUG");
    }

    #[test]
    fn wine_detection_none() {
        clear_wine_env();
        assert!(!is_running_under_wine());
    }
}

// (rest of file unchanged; ShellExecute, FFI, etc. omitted for brevity in this patch)
