//! Bounded subprocess I/O. No pipe reader or stdin writer can outlive a deadline.
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

static SHUTDOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static TERMINATED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
extern "C" fn on_signal(_: libc::c_int) {
    TERMINATED.store(true, std::sync::atomic::Ordering::SeqCst);
}
pub fn install_signal_handlers() {
    // The handler only stores a lock-free flag. AppKit work happens in a thread.
    unsafe {
        libc::signal(libc::SIGTERM, on_signal as libc::sighandler_t);
        libc::signal(libc::SIGINT, on_signal as libc::sighandler_t);
    }
}
pub fn terminated() -> bool {
    TERMINATED.load(std::sync::atomic::Ordering::SeqCst)
}
pub fn running() -> bool {
    !SHUTDOWN.load(std::sync::atomic::Ordering::SeqCst)
}
pub fn shutdown() {
    SHUTDOWN.store(true, std::sync::atomic::Ordering::SeqCst);
}
fn check_running() -> io::Result<()> {
    if SHUTDOWN.load(std::sync::atomic::Ordering::SeqCst) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "application shutting down",
        ))
    } else {
        Ok(())
    }
}

const MAX_BYTES: usize = 4 * 1024 * 1024;
const IO_TICK: Duration = Duration::from_millis(10);

fn nonblocking(fd: &impl AsRawFd) -> io::Result<()> {
    // The descriptor is borrowed and stays open throughout both fcntl calls.
    let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) };
    if flags < 0
        || unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn drain(reader: &mut impl Read, buffer: &mut Vec<u8>) -> io::Result<()> {
    let mut chunk = [0; 8192];
    // A continuously noisy child must not monopolise this loop.
    for _ in 0..16 {
        match reader.read(&mut chunk) {
            Ok(0) => return Ok(()),
            Ok(n) => {
                if buffer.len().saturating_add(n) > MAX_BYTES {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "process output exceeds 4 MiB",
                    ));
                }
                buffer.extend_from_slice(&chunk[..n]);
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

pub struct ChildProcess {
    child: Child,
    input: Option<ChildStdin>,
    output: ChildStdout,
    error: Option<ChildStderr>,
    lines: Vec<u8>,
}

impl ChildProcess {
    pub fn spawn(mut command: Command, capture_stderr: bool) -> io::Result<Self> {
        check_running()?;
        command
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(if capture_stderr {
                Stdio::piped()
            } else {
                Stdio::null()
            });
        let mut child = command.spawn()?;
        // Piped handles are guaranteed by Command. Install the owner before
        // fallible fcntl calls so errors also clean up the process group.
        let input = child.stdin.take();
        let output = child.stdout.take().expect("piped stdout");
        let error = child.stderr.take();
        let owned = Self {
            child,
            input,
            output,
            error,
            lines: Vec::new(),
        };
        if let Some(input) = &owned.input {
            nonblocking(input)?;
        }
        nonblocking(&owned.output)?;
        if let Some(error) = &owned.error {
            nonblocking(error)?;
        }
        Ok(owned)
    }

    fn kill_group(&mut self) {
        // Only the new group created by process_group(0), never the caller's.
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        let _ = self.child.kill();
    }

    pub fn send_json(&mut self, message: &Value, timeout: Duration) -> io::Result<()> {
        let mut bytes = serde_json::to_vec(message)?;
        bytes.push(b'\n');
        if bytes.len() > 256 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "control request exceeds 256 KiB",
            ));
        }
        let deadline = Instant::now() + timeout;
        let mut written = 0;
        while written < bytes.len() {
            check_running()?;
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "stdin write timed out",
                ));
            }
            let input = self
                .input
                .as_mut()
                .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "stdin closed"))?;
            match input.write(&bytes[written..]) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(n) => written += n,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    drain(&mut self.output, &mut self.lines)?;
                    std::thread::sleep(IO_TICK);
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    /// Return one JSON message, preserving every other message for later calls.
    pub fn receive_json(&mut self, timeout: Duration) -> io::Result<Option<Value>> {
        let deadline = Instant::now() + timeout;
        loop {
            check_running()?;
            drain(&mut self.output, &mut self.lines)?;
            for _ in 0..128 {
                let Some(end) = self.lines.iter().position(|b| *b == b'\n') else {
                    break;
                };
                let line: Vec<_> = self.lines.drain(..=end).collect();
                if let Ok(value) = serde_json::from_slice::<Value>(&line) {
                    return Ok(Some(value));
                }
            }
            if self.child.try_wait()?.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "process exited before answering",
                ));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            std::thread::sleep(IO_TICK.min(deadline.saturating_duration_since(Instant::now())));
        }
    }
}

impl Drop for ChildProcess {
    fn drop(&mut self) {
        self.input.take();
        self.kill_group();
        let _ = self.child.wait();
    }
}

pub struct CommandOutput {
    pub stdout: String,
    pub timed_out: bool,
    pub status: Option<ExitStatus>,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        !self.timed_out && self.status.is_some_and(|s| s.success())
    }
}

pub fn run_bounded(
    command: Command,
    input: Option<&str>,
    timeout: Duration,
) -> io::Result<CommandOutput> {
    let deadline = Instant::now() + timeout;
    let mut process = ChildProcess::spawn(command, true)?;
    let bytes = input.unwrap_or_default().as_bytes();
    let mut written = 0;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut status;
    let mut timed_out = false;
    loop {
        check_running()?;
        if written == bytes.len() {
            process.input.take();
        }
        if let Some(stdin) = process.input.as_mut() {
            match stdin.write(&bytes[written..]) {
                Ok(0) => {
                    process.input.take();
                }
                Ok(n) => written += n,
                Err(e)
                    if e.kind() == io::ErrorKind::WouldBlock
                        || e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) if e.kind() == io::ErrorKind::BrokenPipe => {
                    process.input.take();
                }
                Err(e) => return Err(e),
            }
        }
        drain(&mut process.output, &mut stdout)?;
        if let Some(error) = process.error.as_mut() {
            drain(error, &mut stderr)?;
        }
        status = process.child.try_wait()?;
        if status.is_some() {
            // Descendants may still hold stdout/stderr even after their parent
            // exits. Kill the owned group and drain bytes, without waiting for EOF.
            process.kill_group();
            drain(&mut process.output, &mut stdout)?;
            if let Some(error) = process.error.as_mut() {
                drain(error, &mut stderr)?;
            }
            break;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            break;
        }
        std::thread::sleep(IO_TICK);
    }
    Ok(CommandOutput {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        timed_out,
        status,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn shell(script: &str) -> Command {
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", script]);
        cmd
    }

    #[test]
    fn timeout_includes_descendants_holding_pipes() {
        let started = Instant::now();
        let output =
            run_bounded(shell("sleep 10 & wait"), None, Duration::from_millis(120)).unwrap();
        assert!(output.timed_out);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn exited_parent_with_background_child_does_not_wait_for_eof() {
        let started = Instant::now();
        let output =
            run_bounded(shell("sleep 10 & printf ok"), None, Duration::from_secs(1)).unwrap();
        assert_eq!(output.stdout, "ok");
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn a_child_that_never_reads_stdin_is_still_bounded() {
        let started = Instant::now();
        let output = run_bounded(
            shell("sleep 10"),
            Some(&"x".repeat(1024 * 1024)),
            Duration::from_millis(120),
        )
        .unwrap();
        assert!(output.timed_out);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn a_nonzero_exit_is_not_success_even_with_valid_stdout() {
        let output = run_bounded(
            shell("printf '{\"percent\":50}'; exit 7"),
            None,
            Duration::from_secs(1),
        )
        .unwrap();
        assert!(!output.success());
        assert_eq!(output.status.unwrap().code(), Some(7));
    }

    #[test]
    fn excessive_output_is_rejected() {
        let result = run_bounded(
            shell("head -c 5000000 /dev/zero"),
            None,
            Duration::from_secs(2),
        );
        assert_eq!(result.err().unwrap().kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn messages_in_one_read_are_not_lost() {
        let mut child = ChildProcess::spawn(
            shell("printf 'noise\\n{\"id\":1}\\n{\"id\":2}\\n'; sleep 1"),
            false,
        )
        .unwrap();
        assert_eq!(
            child.receive_json(Duration::from_secs(1)).unwrap().unwrap()["id"],
            1
        );
        assert_eq!(
            child.receive_json(Duration::ZERO).unwrap().unwrap()["id"],
            2
        );
    }
}
