//! The channel between the app and the helper, on Windows.
//!
//! - The **app** creates a named pipe with a random name that only the current Windows user
//!   can open, starts the helper (with UAC) and waits for it to connect. It then checks that
//!   the process on the other end is the helper it started.
//! - The **helper** connects without letting the other end impersonate it, checks that the
//!   pipe belongs to the process that started it and to the same Windows user, and only then
//!   reads the request.

use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::path::Path;
use std::process::{Child, Command};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, LocalFree, ERROR_CANCELLED, ERROR_PIPE_CONNECTED, HANDLE, HLOCAL, WAIT_OBJECT_0,
};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{
    EqualSid, GetTokenInformation, TokenUser, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
    TOKEN_QUERY, TOKEN_USER,
};
use windows::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeClientProcessId, GetNamedPipeServerProcessId,
    PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetExitCodeProcess, GetProcessId, OpenProcess, OpenProcessToken,
    TerminateProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Shell::{
    ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

use crate::protocol::{Event, Request, MAX_REQUEST, VERSION};

/// Every pipe name starts with this; the helper refuses any other.
pub const PIPE_PREFIX: &str = r"\\.\pipe\pco-helper-";

/// `SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION`: the pipe's owner may learn who we are,
/// but may not act as us.
const NO_IMPERSONATION: u32 = 0x0010_0000 | 0x0001_0000;

const ERROR_FILE_NOT_FOUND: i32 = 2;
const ERROR_PIPE_BUSY: i32 = 231;

fn os_err(e: windows::core::Error) -> io::Error {
    io::Error::from_raw_os_error(e.code().0 & 0xFFFF)
}

struct SendHandle(HANDLE);
// SAFETY: a pipe handle may be used from any thread; only one thread uses it at a time.
unsafe impl Send for SendHandle {}

/// A random, unguessable pipe name.
fn random_name() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    // Each `RandomState` is seeded from the operating system's random source.
    let part = || RandomState::new().build_hasher().finish();
    format!(
        "{PIPE_PREFIX}{}-{:016x}{:016x}",
        std::process::id(),
        part(),
        part()
    )
}

/// The current Windows user's SID, as text (`S-1-5-21-…`).
pub fn current_user_sid() -> io::Result<String> {
    let token = process_token(None)?;
    let user = token_user(&token)?;
    let mut text = PWSTR::null();
    // SAFETY: `user` holds a TOKEN_USER whose SID pointer stays valid while `user` lives.
    unsafe {
        let info = &*(user.as_ptr() as *const TOKEN_USER);
        ConvertSidToStringSidW(info.User.Sid, &mut text).map_err(os_err)?;
        let s = text.to_string().map_err(|_| io::Error::other("SID"));
        let _ = LocalFree(Some(HLOCAL(text.0.cast())));
        s
    }
}

/// The token of `pid`, or of this process.
fn process_token(pid: Option<u32>) -> io::Result<OwnedHandle> {
    // SAFETY: plain handle-returning calls; every handle is owned and closed exactly once.
    unsafe {
        let process = match pid {
            Some(pid) => {
                let h =
                    OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).map_err(os_err)?;
                Some(OwnedHandle::from_raw_handle(h.0 as RawHandle))
            }
            None => None,
        };
        let process_handle = process
            .as_ref()
            .map_or_else(|| GetCurrentProcess(), |p| HANDLE(p.as_raw_handle()));
        let mut token = HANDLE::default();
        OpenProcessToken(process_handle, TOKEN_QUERY, &mut token).map_err(os_err)?;
        Ok(OwnedHandle::from_raw_handle(token.0 as RawHandle))
    }
}

/// The TOKEN_USER of a token, in an aligned buffer.
fn token_user(token: &OwnedHandle) -> io::Result<Vec<u64>> {
    let handle = HANDLE(token.as_raw_handle());
    let mut needed = 0u32;
    // SAFETY: the first call only asks for the size; the second writes at most `needed` bytes.
    unsafe {
        let _ = GetTokenInformation(handle, TokenUser, None, 0, &mut needed);
        let mut buf = vec![0u64; (needed as usize).div_ceil(8).max(1)];
        GetTokenInformation(
            handle,
            TokenUser,
            Some(buf.as_mut_ptr().cast()),
            needed,
            &mut needed,
        )
        .map_err(os_err)?;
        Ok(buf)
    }
}

/// True if process `pid` runs under the same Windows user as this process.
pub fn same_user_as(pid: u32) -> io::Result<bool> {
    let mine = token_user(&process_token(None)?)?;
    let theirs = token_user(&process_token(Some(pid))?)?;
    // SAFETY: both buffers hold TOKEN_USER structures with valid SID pointers.
    unsafe {
        let a = &*(mine.as_ptr() as *const TOKEN_USER);
        let b = &*(theirs.as_ptr() as *const TOKEN_USER);
        Ok(EqualSid(a.User.Sid, b.User.Sid).is_ok())
    }
}

/// One end of a connected pipe: whole lines in, whole lines out.
pub struct Connection {
    reader: BufReader<File>,
    writer: File,
}

impl Connection {
    fn new(file: File) -> io::Result<Self> {
        let writer = file.try_clone()?;
        Ok(Self {
            reader: BufReader::new(file),
            writer,
        })
    }

    fn raw(&self) -> HANDLE {
        HANDLE(self.writer.as_raw_handle())
    }

    /// The process that created the pipe (seen from the helper).
    pub fn server_pid(&self) -> io::Result<u32> {
        let mut pid = 0u32;
        // SAFETY: `raw()` is a valid pipe handle for the life of `self`.
        unsafe { GetNamedPipeServerProcessId(self.raw(), &mut pid) }.map_err(os_err)?;
        Ok(pid)
    }

    /// The process that connected to the pipe (seen from the app).
    pub fn client_pid(&self) -> io::Result<u32> {
        let mut pid = 0u32;
        // SAFETY: as above.
        unsafe { GetNamedPipeClientProcessId(self.raw(), &mut pid) }.map_err(os_err)?;
        Ok(pid)
    }

    /// The next line without its newline; `None` at the end. Longer than `max` → error.
    pub fn read_line(&mut self, max: usize) -> io::Result<Option<String>> {
        let mut line = String::new();
        let n = (&mut self.reader)
            .take(max as u64 + 2)
            .read_line(&mut line)?;
        if n == 0 {
            return Ok(None);
        }
        if line.len() > max + 1 && !line.ends_with('\n') {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "line too long"));
        }
        Ok(Some(line.trim_end_matches(['\r', '\n']).to_owned()))
    }

    pub fn write_line(&mut self, line: &str) -> io::Result<()> {
        self.writer.write_all(line.as_bytes())?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()
    }
}

/// The app's end, before the helper connects.
pub struct Server {
    name: String,
    handle: HANDLE,
}

impl Drop for Server {
    fn drop(&mut self) {
        if !self.handle.is_invalid() {
            // SAFETY: the handle is owned by this value and not used after this.
            let _ = unsafe { CloseHandle(self.handle) };
        }
    }
}

impl Server {
    /// A new pipe with a random name, open only to the current Windows user, local only.
    pub fn create() -> io::Result<Self> {
        let sddl = HSTRING::from(format!("D:P(A;;GA;;;{})", current_user_sid()?));
        let name = random_name();
        // SAFETY: the security descriptor lives until LocalFree below, after the pipe exists.
        unsafe {
            let mut sd = PSECURITY_DESCRIPTOR::default();
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(sddl.as_ptr()),
                SDDL_REVISION_1,
                &mut sd,
                None,
            )
            .map_err(os_err)?;
            let attrs = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: sd.0,
                bInheritHandle: false.into(),
            };
            let handle = CreateNamedPipeW(
                &HSTRING::from(name.as_str()),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                MAX_REQUEST as u32,
                MAX_REQUEST as u32,
                0,
                Some(&attrs),
            );
            let _ = LocalFree(Some(HLOCAL(sd.0)));
            if handle.is_invalid() {
                return Err(io::Error::last_os_error());
            }
            Ok(Self { name, handle })
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Waits for one client. Gives up after `timeout`, or as soon as `alive()` says the
    /// helper is gone.
    pub fn accept(
        mut self,
        timeout: Duration,
        alive: &mut dyn FnMut() -> bool,
    ) -> io::Result<Connection> {
        let handle = SendHandle(self.handle);
        let (tx, rx) = mpsc::channel();
        let waiter = std::thread::spawn(move || {
            let h = handle;
            // SAFETY: the handle stays open until this thread is joined.
            let r = unsafe { ConnectNamedPipe(h.0, None) };
            let ok = match r {
                Ok(()) => true,
                Err(e) => (e.code().0 & 0xFFFF) as u32 == ERROR_PIPE_CONNECTED.0,
            };
            let _ = tx.send(ok);
        });
        let deadline = Instant::now() + timeout;
        let connected = loop {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(ok) => break ok,
                Err(mpsc::RecvTimeoutError::Disconnected) => break false,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if Instant::now() >= deadline || !alive() {
                        // Unblock the waiting thread with a throw-away connection of our own.
                        let _ = OpenOptions::new().read(true).write(true).open(&self.name);
                        let _ = rx.recv_timeout(Duration::from_secs(5));
                        break false;
                    }
                }
            }
        };
        let _ = waiter.join();
        if !connected {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "the helper did not connect",
            ));
        }
        // SAFETY: ownership of the pipe handle moves into the `File`.
        let file = unsafe { File::from_raw_handle(self.handle.0 as RawHandle) };
        self.handle = HANDLE::default();
        let conn = Connection::new(file)?;
        Ok(conn)
    }
}

/// The helper's end: opens the app's pipe, retrying while it is not there yet.
pub fn connect(name: &str, timeout: Duration) -> io::Result<Connection> {
    if !name.starts_with(PIPE_PREFIX) || name.len() > 200 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a helper pipe",
        ));
    }
    let deadline = Instant::now() + timeout;
    loop {
        match OpenOptions::new()
            .read(true)
            .write(true)
            .security_qos_flags(NO_IMPERSONATION)
            .open(name)
        {
            Ok(file) => return Connection::new(file),
            Err(e)
                if matches!(
                    e.raw_os_error(),
                    Some(ERROR_FILE_NOT_FOUND | ERROR_PIPE_BUSY)
                ) && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(e),
        }
    }
}

/// A started helper process.
pub enum HelperProcess {
    Child(Child),
    Elevated { handle: OwnedHandle, pid: u32 },
}

impl HelperProcess {
    pub fn pid(&self) -> u32 {
        match self {
            HelperProcess::Child(c) => c.id(),
            HelperProcess::Elevated { pid, .. } => *pid,
        }
    }

    fn handle(&self) -> HANDLE {
        match self {
            HelperProcess::Child(c) => HANDLE(c.as_raw_handle()),
            HelperProcess::Elevated { handle, .. } => HANDLE(handle.as_raw_handle()),
        }
    }

    /// The exit code once the process has ended.
    pub fn wait(&self, timeout: Duration) -> Option<u32> {
        let ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
        // SAFETY: the handle is valid while `self` lives.
        unsafe {
            if WaitForSingleObject(self.handle(), ms) != WAIT_OBJECT_0 {
                return None;
            }
            let mut code = 0u32;
            GetExitCodeProcess(self.handle(), &mut code).ok()?;
            Some(code)
        }
    }

    pub fn running(&self) -> bool {
        self.wait(Duration::ZERO).is_none()
    }

    pub fn kill(&self) {
        // SAFETY: as above.
        let _ = unsafe { TerminateProcess(self.handle(), 1) };
    }
}

/// Starts the helper: with the UAC prompt when `elevate`, otherwise as a plain child (tests
/// and CI, where the caller is already an administrator).
pub fn launch(helper: &Path, args: &[String], elevate: bool) -> Result<HelperProcess, RunError> {
    if !elevate {
        return Command::new(helper)
            .args(args)
            .spawn()
            .map(HelperProcess::Child)
            .map_err(|e| RunError::Launch(e.to_string()));
    }
    let file = HSTRING::from(helper.as_os_str());
    let params = HSTRING::from(
        args.iter()
            .map(|a| format!("\"{a}\""))
            .collect::<Vec<_>>()
            .join(" "),
    );
    let verb = HSTRING::from("runas");
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    // SAFETY: every string outlives the call; the process handle is owned afterwards.
    unsafe {
        if let Err(e) = ShellExecuteExW(&mut info) {
            if (e.code().0 & 0xFFFF) as u32 == ERROR_CANCELLED.0 {
                return Err(RunError::Cancelled);
            }
            return Err(RunError::Launch(e.message()));
        }
        if info.hProcess.is_invalid() {
            return Err(RunError::Launch("no process handle".into()));
        }
        let pid = GetProcessId(info.hProcess);
        Ok(HelperProcess::Elevated {
            handle: OwnedHandle::from_raw_handle(info.hProcess.0 as RawHandle),
            pid,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunError {
    /// The user said "No" at the UAC prompt. Nothing was changed.
    Cancelled,
    Launch(String),
    /// The helper never connected (or exited first).
    NoConnection,
    /// Something other than the helper we started connected.
    WrongPeer,
    Io(String),
    /// The helper sent something that is not an event, or stopped before its last event.
    Protocol(String),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::Cancelled => f.write_str("cancelled at the UAC prompt"),
            RunError::Launch(e) => write!(f, "cannot start the helper: {e}"),
            RunError::NoConnection => f.write_str("the helper did not connect"),
            RunError::WrongPeer => f.write_str("an unexpected process connected"),
            RunError::Io(e) => write!(f, "channel error: {e}"),
            RunError::Protocol(e) => write!(f, "unexpected answer from the helper: {e}"),
        }
    }
}

impl std::error::Error for RunError {}

/// The app's side of one request: start the helper, send `request`, pass every event to
/// `on_event` until the last one.
pub fn run(
    helper: &Path,
    request: &Request,
    elevate: bool,
    on_event: &mut dyn FnMut(&Event),
) -> Result<(), RunError> {
    let server = Server::create().map_err(|e| RunError::Io(e.to_string()))?;
    let args = vec![
        "--pipe".to_owned(),
        server.name().to_owned(),
        "--parent".to_owned(),
        std::process::id().to_string(),
    ];
    let process = launch(helper, &args, elevate)?;
    let result = exchange(server, &process, request, on_event);
    if result.is_err() {
        process.kill();
    }
    let _ = process.wait(Duration::from_secs(30));
    result
}

/// Waits for `process` to connect to `server`, checks it, sends `request` and passes every
/// event to `on_event` until the last one.
/// event to on_event until the last one.
pub fn exchange(
    server: Server,
    process: &HelperProcess,
    request: &Request,
    on_event: &mut dyn FnMut(&Event),
) -> Result<(), RunError> {
    let mut conn = server
        .accept(Duration::from_secs(120), &mut || process.running())
        .map_err(|_| RunError::NoConnection)?;
    if conn.client_pid().ok() != Some(process.pid()) {
        return Err(RunError::WrongPeer);
    }
    let io = |e: io::Error| RunError::Io(e.to_string());
    let first = conn
        .read_line(MAX_REQUEST)
        .map_err(io)?
        .and_then(|l| Event::from_line(&l))
        .ok_or_else(|| RunError::Protocol("no hello".into()))?;
    match first {
        Event::Hello { version } if version == VERSION => {}
        Event::Hello { version } => {
            return Err(RunError::Protocol(format!("helper version {version}")))
        }
        other if other.is_final() => {
            on_event(&other);
            return Ok(());
        }
        other => return Err(RunError::Protocol(format!("{other:?}"))),
    }
    conn.write_line(&request.to_line()).map_err(io)?;
    loop {
        let Some(line) = conn.read_line(MAX_REQUEST).map_err(io)? else {
            return Err(RunError::Protocol("the helper stopped early".into()));
        };
        let event = Event::from_line(&line).ok_or_else(|| RunError::Protocol(line.clone()))?;
        on_event(&event);
        if event.is_final() {
            return Ok(());
        }
    }
}
