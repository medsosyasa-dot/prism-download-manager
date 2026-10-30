//! Clipboard monitor: notices copied download links (by file extension) and magnet links.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Extract downloadable URLs from clipboard text.
///
/// A URL qualifies when its path ends with one of `extensions`, or it is a magnet link.
/// At most 200 URLs are returned (pasting a huge list is the batch dialog's job).
pub fn extract_urls(text: &str, extensions: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for token in
        text.split(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '<' || c == '>')
    {
        let token = token.trim_matches(|c: char| matches!(c, ',' | ';' | ')' | '(' | '[' | ']'));
        if token.starts_with("magnet:?") {
            out.push(token.to_string());
        } else if (token.starts_with("http://")
            || token.starts_with("https://")
            || token.starts_with("ftp://"))
            && url::Url::parse(token).is_ok()
        {
            let path = token.split(['?', '#']).next().unwrap_or("");
            let last = path.rsplit('/').next().unwrap_or("");
            if let Some((_, ext)) = last.rsplit_once('.')
                && extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
            {
                out.push(token.to_string());
            }
        }
        if out.len() >= 200 {
            break;
        }
    }
    out.dedup();
    out
}

/// Background clipboard poller. Dropping the handle stops it.
pub struct ClipboardMonitor {
    stop: Arc<AtomicBool>,
}

impl ClipboardMonitor {
    /// Spawn the poller. `extensions` is re-read on every change so settings apply live.
    pub fn spawn<E, F>(extensions: E, on_urls: F) -> Self
    where
        E: Fn() -> Option<Vec<String>> + Send + 'static,
        F: Fn(Vec<String>) + Send + 'static,
    {
        let stop = Arc::new(AtomicBool::new(false));
        // Android doesn't let a background app read the clipboard: no monitor there.
        #[cfg(not(target_os = "android"))]
        spawn_poller(stop.clone(), extensions, on_urls);
        #[cfg(target_os = "android")]
        let _ = (&extensions, &on_urls);
        Self { stop }
    }
}

#[cfg(not(target_os = "android"))]
fn spawn_poller<E, F>(flag: Arc<AtomicBool>, extensions: E, on_urls: F)
where
    E: Fn() -> Option<Vec<String>> + Send + 'static,
    F: Fn(Vec<String>) + Send + 'static,
{
    std::thread::Builder::new()
        .name("prism-clipboard".into())
        .spawn(move || {
            let Ok(mut clipboard) = arboard::Clipboard::new() else {
                tracing::warn!("clipboard unavailable; monitor disabled");
                return;
            };
            // Ignore whatever is on the clipboard at startup.
            let mut last = clipboard.get_text().unwrap_or_default();
            let mut stamp = change_stamp();
            while !flag.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(700));
                // Only read the contents after the system says they changed, and never
                // read what password managers mark as concealed.
                let now = change_stamp();
                if now.is_some() && now == stamp {
                    continue;
                }
                if is_concealed() {
                    stamp = now;
                    continue;
                }
                // Another app may hold the clipboard for a moment: retry on the next tick.
                let Ok(text) = clipboard.get_text() else {
                    continue;
                };
                stamp = now;
                if text == last {
                    continue;
                }
                last = text;
                // `None` means monitoring is switched off in settings right now.
                let Some(exts) = extensions() else { continue };
                let urls = extract_urls(&last, &exts);
                if !urls.is_empty() {
                    on_urls(urls);
                }
            }
        })
        .expect("spawn clipboard thread");
}

/// A counter the OS bumps on every clipboard change (`None`: not available, read every time).
#[cfg(windows)]
fn change_stamp() -> Option<i64> {
    // SAFETY: no arguments; returns 0 when the caller has no window station access.
    let n = unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() };
    (n != 0).then_some(n as i64)
}

#[cfg(target_os = "macos")]
#[allow(unused_unsafe)]
fn change_stamp() -> Option<i64> {
    let pb = unsafe { objc2_app_kit::NSPasteboard::generalPasteboard() };
    Some(unsafe { pb.changeCount() } as i64)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn change_stamp() -> Option<i64> {
    None
}

/// Content marked secret by password managers (nspasteboard.org / Windows exclusion format).
#[cfg(windows)]
fn is_concealed() -> bool {
    use windows_sys::Win32::System::DataExchange::{
        IsClipboardFormatAvailable, RegisterClipboardFormatW,
    };
    let name: Vec<u16> = "ExcludeClipboardContentFromMonitorProcessing\0"
        .encode_utf16()
        .collect();
    // SAFETY: NUL-terminated UTF-16 string; both calls only query clipboard metadata.
    unsafe {
        let fmt = RegisterClipboardFormatW(name.as_ptr());
        fmt != 0 && IsClipboardFormatAvailable(fmt) != 0
    }
}

#[cfg(target_os = "macos")]
#[allow(unused_unsafe)]
fn is_concealed() -> bool {
    let pb = unsafe { objc2_app_kit::NSPasteboard::generalPasteboard() };
    let Some(types) = (unsafe { pb.types() }) else {
        return false;
    };
    types.iter().any(|t| {
        let t = t.to_string();
        t == "org.nspasteboard.ConcealedType" || t == "org.nspasteboard.TransientType"
    })
}

#[cfg(not(any(windows, target_os = "macos")))]
fn is_concealed() -> bool {
    false
}

impl Drop for ClipboardMonitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_matching_links() {
        let exts = vec!["zip".to_string(), "mp4".to_string()];
        let text = "see https://ex.com/a.zip, and (https://ex.com/v.MP4?x=1) not https://ex.com/page.html \
                    magnet:?xt=urn:btih:abc";
        assert_eq!(
            extract_urls(text, &exts),
            vec![
                "https://ex.com/a.zip",
                "https://ex.com/v.MP4?x=1",
                "magnet:?xt=urn:btih:abc"
            ]
        );
        assert!(extract_urls("hello world", &exts).is_empty());
    }
}
