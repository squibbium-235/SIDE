use dioxus::prelude::*;
use once_cell::sync::OnceCell;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::{
    io::{Read, Write},
    sync::{
        mpsc::{self, Sender},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

struct TerminalRuntime {
    input_tx: Sender<String>,
    output: Arc<Mutex<String>>,
}

/// Global PTY-backed terminal runtime (lazy started).
static TERM: OnceCell<TerminalRuntime> = OnceCell::new();

#[cfg(windows)]
fn build_shell_command() -> portable_pty::CommandBuilder {
    use std::env;

    // Prefer COMSPEC (correct cmd.exe), fallback to absolute path
    let comspec = env::var("COMSPEC")
        .unwrap_or_else(|_| r"C:\Windows\System32\cmd.exe".to_string());

    let mut cmd = build_shell_command();

    // Interactive session
    cmd.arg("/Q");
    cmd.arg("/K");

    // Make sure cmd can find Windows DLLs + system stuff
    let system_root = env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    cmd.env("SystemRoot", &system_root);
    cmd.env("ComSpec", env::var("COMSPEC").unwrap_or_else(|_| r"C:\Windows\System32\cmd.exe".to_string()));

    // Optional but often helps if PATH is weird in packaged apps
    if let Ok(path) = env::var("Path") {
        cmd.env("Path", path);
    }

    cmd
}

#[cfg(not(windows))]
fn build_shell_command() -> portable_pty::CommandBuilder {
    use std::env;
    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    let mut cmd = portable_pty::CommandBuilder::new(shell);
    cmd.arg("-l"); // login-ish shell
    cmd
}


fn detect_shell() -> (String, Vec<String>) {
    #[cfg(windows)]
    {
        // cmd.exe is universally present; PowerShell isn't always in PATH on stripped systems.
        ("cmd.exe".to_string(), vec!["/K".to_string()])
    }

    #[cfg(not(windows))]
    {
        let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        (shell, vec!["-l".to_string()])
    }
}

pub fn ensure_started() {
    if TERM.get().is_some() {
        return;
    }

    let pty_system = native_pty_system();
    let pair = match pty_system.openpty(PtySize {
        rows: 30,
        cols: 120,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(p) => p,
        Err(err) => {
            // Can't do much; store error in output so UI shows something.
            let (tx, _rx) = mpsc::channel::<String>();
            let out = Arc::new(Mutex::new(format!("Failed to start PTY: {err}\n")));
            let _ = TERM.set(TerminalRuntime { input_tx: tx, output: out });
            return;
        }
    };

    let (shell, args) = detect_shell();

    let mut cmd = CommandBuilder::new(shell);
    for a in args {
        cmd.arg(a);
    }

    let mut child = match pair.slave.spawn_command(cmd) {
        Ok(c) => c,
        Err(err) => {
            let (tx, _rx) = mpsc::channel::<String>();
            let out = Arc::new(Mutex::new(format!("Failed to spawn shell: {err}\n")));
            let _ = TERM.set(TerminalRuntime { input_tx: tx, output: out });
            return;
        }
    };

    // Keep child alive in a detached thread (drop only when process ends)
    thread::spawn(move || {
        let _ = child.wait();
    });

    let mut reader = match pair.master.try_clone_reader() {
        Ok(r) => r,
        Err(_) => return,
    };
    let mut writer = match pair.master.take_writer() {
        Ok(w) => w,
        Err(_) => return,
    };

    let (input_tx, input_rx) = mpsc::channel::<String>();
    let output = Arc::new(Mutex::new(String::new()));

    // Output reader thread
    {
        let out = output.clone();
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = String::from_utf8_lossy(&buf[..n]);
                        if let Ok(mut s) = out.lock() {
                            s.push_str(&chunk);
                            // Keep the buffer from growing forever.
                            const MAX: usize = 200_000;
                            if s.len() > MAX {
                                let drain = s.len() - MAX;
                                s.drain(..drain);
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    // Input writer thread
    thread::spawn(move || {
        while let Ok(line) = input_rx.recv() {
            let _ = writer.write_all(line.as_bytes());
            let _ = writer.flush();
        }
    });

    let _ = TERM.set(TerminalRuntime { input_tx, output });
}

pub fn send_line(line: String) {
    ensure_started();
    if let Some(rt) = TERM.get() {
        // Add newline if user didn't.
        let mut l = line;
        if !l.ends_with('\n') {
            l.push('\n');
        }
        let _ = rt.input_tx.send(l);
    }
}

fn output_snapshot() -> String {
    if let Some(rt) = TERM.get() {
        if let Ok(s) = rt.output.lock() {
            return s.clone();
        }
    }
    String::new()
}

#[component]
pub fn TerminalView() -> Element {
    ensure_started();

    let mut input = use_signal(|| String::new());
    let mut tick = use_signal(|| 0u64);

    // Periodic rerender so output updates
    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_millis(120)).await;
            tick.set(tick() + 1);
        }
    });

    let _ = tick(); // read to keep dependency
    let out = output_snapshot();

    rsx! {
        div { class: "terminal-inner",
            div { class: "terminal-output", "{out}" }
            div { class: "terminal-input-row",
                span { class: "terminal-prompt", ">" }
                input {
                    class: "terminal-input",
                    value: "{input()}",
                    oninput: move |e| input.set(e.value()),
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Enter {
                            e.prevent_default();
                            let line = input();
                            input.set(String::new());
                            send_line(line);
                        }
                    }
                }
            }
        }
    }
}
