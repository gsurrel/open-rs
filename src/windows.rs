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

/// Encodes as wide and adds a null character.
#[cfg(feature = "shellexecute-on-windows")]
#[inline]
fn wide<T: AsRef<OsStr>>(input: T) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    input
        .as_ref()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Performs an operation on a specified file.
///
/// <https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shellexecuteexw>
#[allow(non_snake_case)]
#[cfg(feature = "shellexecute-on-windows")]
pub unsafe fn ShellExecuteExW(info: *mut ffi::SHELLEXECUTEINFOW) -> std::io::Result<()> {
    if ffi::ShellExecuteExW(info) == 1 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

// Taken from https://microsoft.github.io/windows-docs-rs/doc/windows/
/// Opens a Windows Explorer window with specified items in a particular folder selected.
///
/// <https://learn.microsoft.com/en-us/windows/win32/api/shlobj_core/nf-shlobj_core-shopenfolderandselectitems>
#[allow(non_snake_case)]
#[cfg(feature = "shellexecute-on-windows")]
pub unsafe fn SHOpenFolderAndSelectItems(
    pidlfolder: *const ffi::ITEMIDLIST,
    apidl: Option<&[*const ffi::ITEMIDLIST]>,
    dwflags: u32,
) -> std::io::Result<()> {
    use std::convert::TryInto;
    match ffi::SHOpenFolderAndSelectItems(
        pidlfolder,
        apidl.map_or(0, |slice| slice.len().try_into().unwrap()),
        apidl.map_or(core::ptr::null(), |slice| slice.as_ptr()),
        dwflags,
    ) {
        0 => Ok(()),
        error_code => Err(std::io::Error::from_raw_os_error(error_code)),
    }
}

#[cfg(feature = "shellexecute-on-windows")]
#[allow(non_snake_case)]
mod ffi {
    pub const SW_SHOWNORMAL: i32 = 1;
    pub const EXPLORE: *const u16 = [101, 120, 112, 108, 111, 114, 101, 0].as_ptr();
    pub const FOLDER: *const u16 = [102, 111, 108, 100, 101, 114, 0].as_ptr();
    #[cfg_attr(not(target_arch = "x86"), repr(C))]
    #[cfg_attr(target_arch = "x86", repr(C, packed(1)))]
    #[allow(clippy::upper_case_acronyms)]
    pub struct SHELLEXECUTEINFOW {
        pub cbSize: u32,
        pub fMask: u32,
        pub hwnd: isize,
        pub lpVerb: *const u16,
        pub lpFile: *const u16,
        pub lpParameters: *const u16,
        pub lpDirectory: *const u16,
        pub nShow: i32,
        pub hInstApp: isize,
        pub lpIDList: *mut core::ffi::c_void,
        pub lpClass: *const u16,
        pub hkeyClass: isize,
        pub dwHotKey: u32,
        pub Anonymous: SHELLEXECUTEINFOW_0,
        pub hProcess: isize,
    }
    #[cfg_attr(not(target_arch = "x86"), repr(C))]
    #[cfg_attr(target_arch = "x86", repr(C, packed(1)))]
    pub union SHELLEXECUTEINFOW_0 {
        pub hIcon: isize,
        pub hMonitor: isize,
    }
    #[repr(C, packed(1))]
    #[allow(clippy::upper_case_acronyms)]
    pub struct SHITEMID {
        pub cb: u16,
        pub abID: [u8; 1],
    }
    #[repr(C, packed(1))]
    #[allow(clippy::upper_case_acronyms)]
    pub struct ITEMIDLIST {
        pub mkid: SHITEMID,
    }
    #[link(name = "shell32")]
    extern "system" {
        pub fn ShellExecuteExW(info: *mut SHELLEXECUTEINFOW) -> isize;
        pub fn ILCreateFromPathW(pszpath: *const u16) -> *mut ITEMIDLIST;
        pub fn SHOpenFolderAndSelectItems(
            pidlfolder: *const ITEMIDLIST,
            cidl: u32,
            apidl: *const *const ITEMIDLIST,
            dwflags: u32,
        ) -> i32;
        pub fn ILFree(pidl: *const ITEMIDLIST) -> i32;
    }
    #[link(name = "ole32")]
    extern "system" {
        pub fn CoInitialize(pvreserved: *const core::ffi::c_void) -> i32;
    }
}
