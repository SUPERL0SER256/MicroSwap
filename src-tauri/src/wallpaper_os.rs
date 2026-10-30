use std::path::{Path, PathBuf};

#[cfg(windows)]
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ,
};
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SystemParametersInfoW, SPIF_SENDWININICHANGE, SPIF_UPDATEINIFILE, SPI_GETDESKWALLPAPER,
    SPI_SETDESKWALLPAPER,
};

#[cfg(windows)]
fn to_wide_null(s: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Detects the current Windows desktop wallpaper path.
/// First tries `SystemParametersInfoW(SPI_GETDESKWALLPAPER)`.
/// If that path is empty or doesn't exist, falls back to `%APPDATA%\Microsoft\Windows\Themes\TranscodedWallpaper`.
pub fn get_current_windows_wallpaper(fallback_copy_dir: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let mut buf = [0u16; 520];
        let ok = unsafe {
            SystemParametersInfoW(
                SPI_GETDESKWALLPAPER,
                buf.len() as u32,
                buf.as_mut_ptr() as *mut _,
                0,
            )
        };
        if ok != 0 {
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            if len > 0 {
                let path_str = String::from_utf16_lossy(&buf[..len]);
                let p = PathBuf::from(&path_str);
                if p.exists() && p.is_file() {
                    // If the path ends with TranscodedWallpaper (no extension), copy it to a .jpg in app data so image crate and asset protocol recognize it cleanly
                    if p.extension().is_none() {
                        let dest = fallback_copy_dir.join("Current_Windows_Wallpaper.jpg");
                        if std::fs::copy(&p, &dest).is_ok() {
                            return Some(dest);
                        }
                    }
                    return Some(p);
                }
            }
        }

        // Fallback: check %APPDATA%\Microsoft\Windows\Themes\TranscodedWallpaper
        if let Ok(appdata) = std::env::var("APPDATA") {
            let transcoded = PathBuf::from(appdata)
                .join("Microsoft")
                .join("Windows")
                .join("Themes")
                .join("TranscodedWallpaper");
            if transcoded.exists() {
                let dest = fallback_copy_dir.join("Current_Windows_Wallpaper.jpg");
                if std::fs::copy(&transcoded, &dest).is_ok() {
                    return Some(dest);
                }
            }
        }
    }

    let _ = fallback_copy_dir;
    None
}

/// Sets the Windows desktop wallpaper and applies the requested scaling style.
pub fn apply_windows_wallpaper(file_path: &str, scaling: &str) -> Result<(), String> {
    let p = Path::new(file_path);
    if !p.exists() {
        return Err(format!("Wallpaper file no longer exists at: {}", file_path));
    }

    #[cfg(windows)]
    {
        // Set registry WallpaperStyle and TileWallpaper first
        let (style, tile) = match scaling.to_lowercase().as_str() {
            "center" => ("0", "0"),
            "tile" => ("0", "1"),
            "stretch" => ("2", "0"),
            "fit" => ("6", "0"),
            "span" => ("22", "0"),
            _ => ("10", "0"), // "fill" default
        };

        unsafe {
            let subkey = to_wide_null("Control Panel\\Desktop");
            let mut hkey: HKEY = std::ptr::null_mut();
            if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_SET_VALUE, &mut hkey) == 0 {
                let style_name = to_wide_null("WallpaperStyle");
                let style_val = to_wide_null(style);
                RegSetValueExW(
                    hkey,
                    style_name.as_ptr(),
                    0,
                    REG_SZ,
                    style_val.as_ptr() as *const u8,
                    (style_val.len() * 2) as u32,
                );

                let tile_name = to_wide_null("TileWallpaper");
                let tile_val = to_wide_null(tile);
                RegSetValueExW(
                    hkey,
                    tile_name.as_ptr(),
                    0,
                    REG_SZ,
                    tile_val.as_ptr() as *const u8,
                    (tile_val.len() * 2) as u32,
                );

                RegCloseKey(hkey);
            }

            let wide_path = to_wide_null(file_path);
            let result = SystemParametersInfoW(
                SPI_SETDESKWALLPAPER,
                0,
                wide_path.as_ptr() as *mut _,
                SPIF_UPDATEINIFILE | SPIF_SENDWININICHANGE,
            );

            if result == 0 {
                return Err("Windows SystemParametersInfoW failed to apply the wallpaper.".into());
            }
        }
    }

    Ok(())
}
