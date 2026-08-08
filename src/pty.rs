#[cfg(unix)]
mod unix {
    use std::{
        collections::VecDeque,
        fs,
        io::{self, BufRead, BufReader, Read, Write},
        os::unix::{
            fs::PermissionsExt,
            net::{UnixListener, UnixStream},
        },
        path::{Path, PathBuf},
        sync::{Arc, Mutex},
        thread,
        time::{SystemTime, UNIX_EPOCH},
    };

    use anyhow::{Context, Result, bail};
    use crossterm::terminal;
    use portable_pty::{CommandBuilder, PtySize, native_pty_system};
    use serde::{Deserialize, Serialize};

    const MAX_CURRENT: usize = 128 * 1024;
    const MAX_RECORDS: usize = 5;

    #[derive(Default)]
    struct State {
        current: VecDeque<u8>,
        records: VecDeque<Record>,
    }

    #[derive(Clone, Serialize, Deserialize)]
    struct Record {
        command: String,
        exit_code: Option<i32>,
        output: String,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(tag = "type", rename_all = "lowercase")]
    enum Request {
        Mark {
            command: String,
            exit_code: Option<i32>,
        },
        Get {
            command: String,
        },
    }

    #[derive(Serialize, Deserialize)]
    struct Response {
        output: Option<String>,
    }

    pub fn run(command: &[String]) -> Result<()> {
        if command.is_empty() {
            bail!("PTY shell command is required");
        }
        let socket = socket_path();
        let listener = UnixListener::bind(&socket)
            .with_context(|| format!("failed to create {}", socket.display()))?;
        fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))?;
        let state = Arc::new(Mutex::new(State::default()));
        let server_state = Arc::clone(&state);
        thread::spawn(move || serve(listener, server_state));

        let (cols, rows) = terminal::size().unwrap_or((80, 24));
        let pair = native_pty_system().openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let mut builder = CommandBuilder::new(&command[0]);
        for arg in &command[1..] {
            builder.arg(arg);
        }
        builder.env("LLMFUCK_PTY_SOCKET", socket.as_os_str());
        builder.env("LLMFUCK_PTY_SESSION", "1");
        let mut child = pair.slave.spawn_command(builder)?;
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader()?;
        let mut writer = pair.master.take_writer()?;
        let input = thread::spawn(move || {
            let mut stdin = io::stdin().lock();
            let _ = io::copy(&mut stdin, &mut writer);
        });
        let _raw = RawMode::enter()?;
        let mut stdout = io::stdout().lock();
        let mut buf = [0u8; 8192];
        loop {
            let read = reader.read(&mut buf)?;
            if read == 0 {
                break;
            }
            stdout.write_all(&buf[..read])?;
            stdout.flush()?;
            let mut guard = state.lock().expect("PTY state lock poisoned");
            for byte in &buf[..read] {
                if guard.current.len() == MAX_CURRENT {
                    guard.current.pop_front();
                }
                guard.current.push_back(*byte);
            }
        }
        let status = child.wait()?;
        let _ = input.thread().id();
        let _ = fs::remove_file(&socket);
        if status.success() {
            Ok(())
        } else {
            bail!("PTY shell exited with status {status:?}")
        }
    }

    pub fn mark(socket: &Path, command: String, exit_code: Option<i32>) -> Result<()> {
        request(socket, &Request::Mark { command, exit_code }).map(|_| ())
    }

    pub fn get(socket: &Path, command: String) -> Result<Option<String>> {
        request(socket, &Request::Get { command }).map(|v| v.output)
    }

    fn request(socket: &Path, request: &Request) -> Result<Response> {
        let mut stream = UnixStream::connect(socket)?;
        serde_json::to_writer(&mut stream, request)?;
        stream.write_all(b"\n")?;
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line)?;
        Ok(serde_json::from_str(&line)?)
    }

    fn serve(listener: UnixListener, state: Arc<Mutex<State>>) {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            let Ok(request) = serde_json::from_str::<Request>(&line) else {
                continue;
            };
            let response = handle(request, &state);
            let stream = reader.get_mut();
            let _ = serde_json::to_writer(&mut *stream, &response);
            let _ = stream.write_all(b"\n");
        }
    }

    fn handle(request: Request, state: &Arc<Mutex<State>>) -> Response {
        let mut state = state.lock().expect("PTY state lock poisoned");
        match request {
            Request::Mark { command, exit_code } => {
                let bytes: Vec<u8> = state.current.drain(..).collect();
                let output = clean_terminal_output(&String::from_utf8_lossy(&bytes));
                if !command.trim().is_empty() {
                    state.records.push_back(Record {
                        command,
                        exit_code,
                        output,
                    });
                    while state.records.len() > MAX_RECORDS {
                        state.records.pop_front();
                    }
                }
                Response { output: None }
            }
            Request::Get { command } => {
                let output = state
                    .records
                    .iter()
                    .rev()
                    .find(|r| normalized(&r.command) == normalized(&command))
                    .or_else(|| state.records.back())
                    .map(|r| r.output.clone());
                Response { output }
            }
        }
    }

    fn clean_terminal_output(value: &str) -> String {
        let mut out = String::with_capacity(value.len());
        let mut chars = value.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\u{1b}' {
                if chars.next_if_eq(&'[').is_some() {
                    for next in chars.by_ref() {
                        if ('@'..='~').contains(&next) {
                            break;
                        }
                    }
                }
                continue;
            }
            if ch != '\r' && (ch == '\n' || ch == '\t' || !ch.is_control()) {
                out.push(ch);
            }
        }
        out.trim().to_string()
    }

    fn normalized(value: &str) -> &str {
        value.trim()
    }

    fn socket_path() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("llmfuck-{}-{nonce}.sock", std::process::id()))
    }

    struct RawMode;
    impl RawMode {
        fn enter() -> Result<Self> {
            terminal::enable_raw_mode()?;
            Ok(Self)
        }
    }
    impl Drop for RawMode {
        fn drop(&mut self) {
            let _ = terminal::disable_raw_mode();
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn strips_ansi_sequences() {
            assert_eq!(clean_terminal_output("\x1b[31merror\x1b[0m\r\n"), "error");
        }

        #[test]
        fn returns_the_matching_completed_record() {
            let state = Arc::new(Mutex::new(State::default()));
            state.lock().unwrap().current.extend(b"failed output");
            handle(
                Request::Mark {
                    command: "git chekout main".into(),
                    exit_code: Some(1),
                },
                &state,
            );
            let response = handle(
                Request::Get {
                    command: "git chekout main".into(),
                },
                &state,
            );
            assert_eq!(response.output.as_deref(), Some("failed output"));
        }
    }
}

#[cfg(unix)]
pub use unix::{get, mark, run};

#[cfg(not(unix))]
pub fn run(_command: &[String]) -> anyhow::Result<()> {
    anyhow::bail!("Windows ConPTY support is planned but not available in this release")
}

#[cfg(not(unix))]
pub fn get(_socket: &std::path::Path, _command: String) -> anyhow::Result<Option<String>> {
    Ok(None)
}

#[cfg(not(unix))]
pub fn mark(
    _socket: &std::path::Path,
    _command: String,
    _exit_code: Option<i32>,
) -> anyhow::Result<()> {
    anyhow::bail!("Windows ConPTY support is planned but not available in this release")
}
