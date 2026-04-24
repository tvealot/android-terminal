use std::io::Write;
use std::process::{Command, Stdio};

pub fn copy(text: &str) -> Result<&'static str, String> {
    for (bin, args) in candidates() {
        match Command::new(bin)
            .args(*args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(mut child) => {
                if let Some(stdin) = child.stdin.as_mut() {
                    if let Err(e) = stdin.write_all(text.as_bytes()) {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(format!("{bin}: write failed: {e}"));
                    }
                }
                drop(child.stdin.take());
                match child.wait() {
                    Ok(status) if status.success() => return Ok(bin),
                    Ok(status) => return Err(format!("{bin} exited with {status}")),
                    Err(e) => return Err(format!("{bin}: {e}")),
                }
            }
            Err(_) => continue,
        }
    }
    Err("no clipboard tool found (pbcopy/xclip/wl-copy/clip)".into())
}

#[cfg(target_os = "macos")]
fn candidates() -> &'static [(&'static str, &'static [&'static str])] {
    &[("pbcopy", &[])]
}

#[cfg(all(unix, not(target_os = "macos")))]
fn candidates() -> &'static [(&'static str, &'static [&'static str])] {
    &[
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["--clipboard", "--input"]),
    ]
}

#[cfg(windows)]
fn candidates() -> &'static [(&'static str, &'static [&'static str])] {
    &[("clip", &[])]
}
