use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

const SCROLLBACK: usize = 2000;

pub struct ShellState {
    pub parser: Arc<Mutex<vt100::Parser>>,
    pub active: bool,
    pub last_error: Option<String>,
    writer: Option<Box<dyn Write + Send>>,
    master: Option<Box<dyn MasterPty + Send>>,
    child: Option<Box<dyn Child + Send + Sync>>,
    size: (u16, u16),
    exit_guard: Arc<Mutex<Option<String>>>,
}

impl Default for ShellState {
    fn default() -> Self {
        Self::new(24, 80)
    }
}

impl ShellState {
    pub fn new(rows: u16, cols: u16) -> Self {
        Self {
            parser: Arc::new(Mutex::new(vt100::Parser::new(rows, cols, SCROLLBACK))),
            active: false,
            last_error: None,
            writer: None,
            master: None,
            child: None,
            size: (rows, cols),
            exit_guard: Arc::new(Mutex::new(None)),
        }
    }

    pub fn start(&mut self, serial: Option<&str>) -> Result<(), String> {
        if self.active {
            return Ok(());
        }
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: self.size.0,
                cols: self.size.1,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string())?;
        let mut cmd = CommandBuilder::new("adb");
        if let Some(s) = serial {
            cmd.arg("-s");
            cmd.arg(s);
        }
        cmd.arg("shell");
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| e.to_string())?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| e.to_string())?;
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| e.to_string())?;

        let parser = self.parser.clone();
        let exit_guard = self.exit_guard.clone();
        thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        if let Ok(mut g) = exit_guard.lock() {
                            *g = Some("shell closed".to_string());
                        }
                        break;
                    }
                    Err(e) => {
                        if let Ok(mut g) = exit_guard.lock() {
                            *g = Some(format!("shell read error: {}", e));
                        }
                        break;
                    }
                    Ok(n) => {
                        if let Ok(mut p) = parser.lock() {
                            p.process(&buf[..n]);
                        }
                    }
                }
            }
        });

        self.writer = Some(writer);
        self.master = Some(pair.master);
        self.child = Some(child);
        self.active = true;
        self.last_error = None;
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        self.writer = None;
        self.master = None;
        self.active = false;
    }

    pub fn write(&mut self, bytes: &[u8]) {
        if let Some(w) = self.writer.as_mut() {
            let _ = w.write_all(bytes);
            let _ = w.flush();
        }
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        if (rows, cols) == self.size || rows == 0 || cols == 0 {
            return;
        }
        self.size = (rows, cols);
        if let Ok(mut p) = self.parser.lock() {
            p.set_size(rows, cols);
        }
        if let Some(m) = self.master.as_ref() {
            let _ = m.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }

    pub fn poll_exit(&mut self) -> Option<String> {
        if !self.active {
            return None;
        }
        let msg = self.exit_guard.lock().ok().and_then(|g| g.clone());
        if let Some(m) = &msg {
            self.last_error = Some(m.clone());
            self.stop();
        }
        msg
    }
}
