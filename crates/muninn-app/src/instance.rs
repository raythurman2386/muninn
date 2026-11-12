//! One running Muninn per user session (#42).
//!
//! A second launch must not start a second app: each process owns its own
//! tray icon, so two processes show two "M" icons, and both would write the
//! same `settings.json`. The first windowed launch takes an exclusive lock
//! on `instance.lock` and listens for "show" requests. A later launch finds
//! the lock taken, hands its request over (the launcher's activation token
//! and its `--vault`), and exits 0. The running app then switches to that
//! vault when one was named and brings its window to the front.
//!
//! The lock is an OS file lock ([`std::fs::File::try_lock`]), the same as the
//! sync lease (`muninn-sync/src/lease.rs`): it is released when the process
//! exits or is killed, so a crash never leaves a stale lock. The channel is
//! a Unix socket on Linux and macOS, in the lock's `0700` folder. A stale
//! socket file is removed only while the lock is held. Windows has no Unix
//! socket in std, so the app listens on `127.0.0.1` and writes the port with
//! a random secret to `instance.info`; a request without the secret is
//! dropped.
//!
//! `--new-instance` skips all of this. Headless runs (`--snap`, `--tree`,
//! `--check`) never take the lock.

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// What a later launch asks the running app to do.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Request {
    /// The launcher's activation token (`XDG_ACTIVATION_TOKEN` on Wayland,
    /// `DESKTOP_STARTUP_ID` on X11), so the compositor lets the existing
    /// window take focus.
    pub token: Option<String>,
    /// The `--vault` the later launch was given, made absolute.
    pub vault: Option<PathBuf>,
}

impl Request {
    /// This process's request: its activation token and `vault`.
    pub fn from_env(vault: Option<PathBuf>) -> Self {
        let token = ["XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID"]
            .iter()
            .find_map(|name| std::env::var(name).ok().filter(|t| !t.is_empty()));
        let vault = vault.map(|v| std::path::absolute(&v).unwrap_or(v));
        Self { token, vault }
    }

    fn encode(&self, secret: &str) -> String {
        let value = serde_json::json!({
            "v": 1,
            "cmd": "show",
            "token": self.token,
            "vault": self.vault.as_ref().map(|v| v.to_string_lossy().into_owned()),
            "secret": secret,
        });
        format!("{value}\n")
    }

    fn decode(line: &str, secret: &str) -> Option<Self> {
        let value: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
        if value.get("v")?.as_u64()? != 1 || value.get("cmd")?.as_str()? != "show" {
            return None;
        }
        if value.get("secret").and_then(serde_json::Value::as_str) != Some(secret) {
            return None;
        }
        let text = |key: &str| {
            value
                .get(key)
                .and_then(serde_json::Value::as_str)
                .filter(|t| !t.is_empty())
                .map(str::to_owned)
        };
        Some(Self {
            token: text("token"),
            vault: text("vault").map(PathBuf::from),
        })
    }
}

/// Where the lock and the channel live.
#[derive(Clone, Debug)]
pub struct Paths {
    dir: PathBuf,
}

/// The longest Unix socket path used as is. `sun_path` is 108 bytes on
/// Linux and 104 on macOS, both counting the terminating NUL.
#[cfg(unix)]
const SOCKET_PATH_MAX: usize = 100;

/// How long a later launch waits for the running app to answer.
pub const ANSWER_WAIT: Duration = Duration::from_secs(3);

/// A request line is small; anything longer is dropped.
const REQUEST_MAX: u64 = 64 * 1024;

impl Paths {
    /// The lock folder in `dir`.
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// `$XDG_RUNTIME_DIR/muninn`, else `$XDG_STATE_HOME/muninn`, else
    /// `~/.local/state/muninn`; `%LOCALAPPDATA%\muninn` on Windows.
    pub fn from_env() -> Option<Self> {
        let var = |name: &str| {
            std::env::var_os(name)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
        };
        let base = if cfg!(windows) {
            var("LOCALAPPDATA")
        } else {
            var("XDG_RUNTIME_DIR")
                .or_else(|| var("XDG_STATE_HOME"))
                .or_else(|| var("HOME").map(|home| home.join(".local").join("state")))
        };
        base.map(|base| Self::at(base.join("muninn")))
    }

    /// The lock file.
    pub fn lock(&self) -> PathBuf {
        self.dir.join("instance.lock")
    }

    /// The running app's pid (and on Windows its port and secret).
    pub fn info(&self) -> PathBuf {
        self.dir.join("instance.info")
    }

    /// The socket: `instance.sock` beside the lock, or, when that path is too
    /// long for `sun_path`, `/tmp/muninn-<uid>-<hash of the folder>/s`.
    #[cfg(unix)]
    pub fn socket(&self) -> PathBuf {
        let near = self.dir.join("instance.sock");
        if near.as_os_str().len() <= SOCKET_PATH_MAX {
            return near;
        }
        self.short_dir().join("s")
    }

    #[cfg(unix)]
    fn short_dir(&self) -> PathBuf {
        Path::new("/tmp").join(format!("muninn-{:016x}", fnv1a(&self.dir)))
    }

    /// When the socket lives in the shared `/tmp` fallback, check that
    /// folder is private and owned by whoever owns the lock folder, so
    /// nobody else can sit on the path first.
    #[cfg(unix)]
    fn check_socket_dir(&self, create: bool) -> Result<(), Error> {
        use std::os::unix::fs::MetadataExt;
        let socket = self.socket();
        let Some(parent) = socket.parent().filter(|p| *p != self.dir) else {
            return Ok(());
        };
        let uid = fs::metadata(&self.dir)
            .map_err(|e| io("could not read", &self.dir, e))?
            .uid();
        if !create && !parent.exists() {
            return Ok(());
        }
        private_dir(parent, Some(uid))
    }
}

/// A stable 64-bit FNV-1a of a path (std's hasher is not stable across runs).
#[cfg(unix)]
fn fnv1a(path: &Path) -> u64 {
    path.as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        })
}

/// Why the single-instance check failed.
#[derive(Debug)]
pub enum Error {
    /// The lock folder, lock or channel could not be set up.
    Io(String),
    /// Another Muninn holds the lock but did not answer in time.
    NoAnswer { pid: Option<u32> },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(detail) => write!(f, "single-instance check failed: {detail}"),
            Self::NoAnswer { pid: Some(pid) } => write!(
                f,
                "another Muninn is running (pid {pid}) but did not answer; \
                 quit it, or start with --new-instance"
            ),
            Self::NoAnswer { pid: None } => write!(
                f,
                "another Muninn is running but did not answer; \
                 quit it, or start with --new-instance"
            ),
        }
    }
}

fn io(context: &str, path: &Path, error: impl std::fmt::Display) -> Error {
    Error::Io(format!("{context} {}: {error}", path.display()))
}

/// The outcome of [`claim`].
pub enum Claim {
    /// This process is the running app: keep this alive while it runs and
    /// call [`Primary::serve`] once the app handle exists.
    Primary(Primary),
    /// The running app took the request; this process should exit 0.
    Forwarded,
}

/// The lock and listener of the running app. Dropping it releases both.
pub struct Primary {
    lock: File,
    listener: Listener,
    secret: String,
    #[cfg(unix)]
    socket: PathBuf,
}

#[cfg(unix)]
type Listener = std::os::unix::net::UnixListener;
#[cfg(not(unix))]
type Listener = std::net::TcpListener;

/// Take the lock, or hand `request` to the app that holds it, waiting up to
/// `wait` for it to answer.
pub fn claim(paths: &Paths, request: &Request, wait: Duration) -> Result<Claim, Error> {
    private_dir(&paths.dir, None)?;
    let lock_path = paths.lock();
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|e| io("could not open", &lock_path, e))?;
    match lock.try_lock() {
        Ok(()) => listen(paths, lock).map(Claim::Primary),
        Err(fs::TryLockError::WouldBlock) => {
            forward(paths, request, wait)?;
            Ok(Claim::Forwarded)
        }
        Err(fs::TryLockError::Error(e)) => Err(io("could not lock", &lock_path, e)),
    }
}

/// Create `dir` (`0700` on Unix) unless it exists, and refuse a folder
/// others can write to. `owner`, when given, must own it: the short socket
/// folder lives in the shared `/tmp`, where anyone could make it first.
fn private_dir(dir: &Path, owner: Option<u32>) -> Result<(), Error> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
        if !dir.is_dir() {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)
                .map_err(|e| io("could not create", dir, e))?;
        }
        let meta = fs::symlink_metadata(dir).map_err(|e| io("could not read", dir, e))?;
        let shared = match owner {
            Some(uid) => meta.uid() != uid || meta.permissions().mode() & 0o077 != 0,
            None => meta.permissions().mode() & 0o022 != 0,
        };
        if !meta.is_dir() || shared {
            return Err(Error::Io(format!(
                "{} must be a folder only you can write to",
                dir.display()
            )));
        }
    }
    #[cfg(not(unix))]
    {
        let _ = owner;
        fs::create_dir_all(dir).map_err(|e| io("could not create", dir, e))?;
    }
    Ok(())
}

#[cfg(unix)]
fn listen(paths: &Paths, lock: File) -> Result<Primary, Error> {
    let socket = paths.socket();
    paths.check_socket_dir(true)?;
    // Holding the lock, so any socket file here is a dead app's.
    match fs::remove_file(&socket) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io("could not remove the stale", &socket, e)),
    }
    let listener = Listener::bind(&socket).map_err(|e| io("could not listen on", &socket, e))?;
    write_info(paths, &format!("{}\n", std::process::id()))?;
    Ok(Primary {
        lock,
        listener,
        secret: String::new(),
        socket,
    })
}

#[cfg(not(unix))]
fn listen(paths: &Paths, lock: File) -> Result<Primary, Error> {
    let listener = Listener::bind(("127.0.0.1", 0))
        .map_err(|e| Error::Io(format!("could not listen on 127.0.0.1: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| Error::Io(format!("could not read the listener port: {e}")))?
        .port();
    let secret = random_secret();
    write_info(
        paths,
        &format!("{}\n{port}\n{secret}\n", std::process::id()),
    )?;
    Ok(Primary {
        lock,
        listener,
        secret,
    })
}

/// Write `instance.info` through a temp file and a rename, so a later launch
/// never reads half of it.
fn write_info(paths: &Paths, text: &str) -> Result<(), Error> {
    let path = paths.info();
    let temp = paths
        .dir
        .join(format!("instance.info.{}", std::process::id()));
    fs::write(&temp, text).map_err(|e| io("could not write", &temp, e))?;
    fs::rename(&temp, &path).map_err(|e| io("could not write", &path, e))
}

/// 128 random bits as hex, from std's randomly seeded hasher keys.
#[cfg(not(unix))]
fn random_secret() -> String {
    use std::hash::BuildHasher;
    let a = std::collections::hash_map::RandomState::new().hash_one(std::process::id());
    let b = std::collections::hash_map::RandomState::new().hash_one(Instant::now());
    format!("{a:016x}{b:016x}")
}

/// The running app's pid from `instance.info`, for the error message.
fn holder_pid(paths: &Paths) -> Option<u32> {
    fs::read_to_string(paths.info())
        .ok()?
        .lines()
        .next()?
        .trim()
        .parse()
        .ok()
}

/// Send `request` and wait for `ok`, retrying while the running app is
/// still starting up (it takes the lock before it listens).
fn forward(paths: &Paths, request: &Request, wait: Duration) -> Result<(), Error> {
    let deadline = Instant::now() + wait;
    loop {
        if send_once(paths, request, deadline).is_ok() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(Error::NoAnswer {
                pid: holder_pid(paths),
            });
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn send_once(paths: &Paths, request: &Request, deadline: Instant) -> std::io::Result<()> {
    let left = deadline
        .saturating_duration_since(Instant::now())
        .max(Duration::from_millis(50));
    #[cfg(unix)]
    paths
        .check_socket_dir(false)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    #[cfg(unix)]
    let (mut stream, secret) = (
        std::os::unix::net::UnixStream::connect(paths.socket())?,
        String::new(),
    );
    #[cfg(not(unix))]
    let (mut stream, secret) = {
        let info = fs::read_to_string(paths.info())?;
        let mut lines = info.lines().skip(1);
        let port: u16 = lines
            .next()
            .and_then(|p| p.trim().parse().ok())
            .ok_or_else(|| std::io::Error::other("no port in instance.info"))?;
        let secret = lines.next().unwrap_or_default().trim().to_owned();
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        (std::net::TcpStream::connect_timeout(&addr, left)?, secret)
    };
    stream.set_read_timeout(Some(left))?;
    stream.set_write_timeout(Some(left))?;
    stream.write_all(request.encode(&secret).as_bytes())?;
    stream.flush()?;
    let mut answer = String::new();
    BufReader::new(stream.take(16)).read_line(&mut answer)?;
    if answer.trim() == "ok" {
        Ok(())
    } else {
        Err(std::io::Error::other("no answer"))
    }
}

impl Primary {
    /// Answer later launches on a background thread: each valid request is
    /// acknowledged, then handed to `on_request`. Invalid ones are dropped.
    pub fn serve(&self, on_request: impl Fn(Request) + Send + 'static) -> Result<(), Error> {
        let listener = self
            .listener
            .try_clone()
            .map_err(|e| Error::Io(format!("could not share the listener: {e}")))?;
        let secret = self.secret.clone();
        std::thread::Builder::new()
            .name("muninn-instance".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { continue };
                    let _ = stream.set_read_timeout(Some(ANSWER_WAIT));
                    let _ = stream.set_write_timeout(Some(ANSWER_WAIT));
                    let mut line = String::new();
                    let read = match stream.try_clone() {
                        Ok(reader) => BufReader::new(reader.take(REQUEST_MAX)).read_line(&mut line),
                        Err(e) => Err(e),
                    };
                    if read.is_err() {
                        continue;
                    }
                    let Some(request) = Request::decode(&line, &secret) else {
                        continue;
                    };
                    let _ = stream.write_all(b"ok\n");
                    drop(stream);
                    on_request(request);
                }
            })
            .map(|_| ())
            .map_err(|e| Error::Io(format!("could not start the listener thread: {e}")))
    }
}

impl Drop for Primary {
    fn drop(&mut self) {
        // Still holding the lock, so the socket file is ours to remove.
        #[cfg(unix)]
        let _ = fs::remove_file(&self.socket);
        // Unlock now rather than when the file closes: a process being
        // spawned at this moment holds a copy of every open file until it
        // execs, and the lock would last until that copy closes too.
        let _ = self.lock.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::mpsc;

    /// A fresh folder in the temp folder (`TMPDIR`), one per test. The name
    /// is short so the socket still fits beside the lock under macOS's long
    /// per-user temp folder.
    fn scratch(name: &str) -> PathBuf {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("mi-{name}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        path
    }

    fn primary(claim: Claim) -> Primary {
        match claim {
            Claim::Primary(p) => p,
            Claim::Forwarded => panic!("expected to be the first instance"),
        }
    }

    #[test]
    fn the_second_claim_is_handed_to_the_first() {
        let dir = scratch("forward");
        let paths = Paths::at(&dir);
        let first = primary(claim(&paths, &Request::default(), ANSWER_WAIT).unwrap());
        let (tx, rx) = mpsc::channel();
        first.serve(move |r| tx.send(r).unwrap()).unwrap();
        let ask = Request {
            token: Some("t".into()),
            vault: Some(dir.join("Other vault")),
        };
        assert!(matches!(
            claim(&paths, &ask, ANSWER_WAIT).unwrap(),
            Claim::Forwarded
        ));
        assert_eq!(rx.recv_timeout(ANSWER_WAIT).unwrap(), ask);
        // A plain relaunch carries no token or vault.
        assert!(matches!(
            claim(&paths, &Request::default(), ANSWER_WAIT).unwrap(),
            Claim::Forwarded
        ));
        assert_eq!(rx.recv_timeout(ANSWER_WAIT).unwrap(), Request::default());
        drop(first);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_dropped_primary_frees_the_lock() {
        let dir = scratch("drop");
        let paths = Paths::at(&dir);
        let first = primary(claim(&paths, &Request::default(), ANSWER_WAIT).unwrap());
        drop(first);
        let again = primary(claim(&paths, &Request::default(), ANSWER_WAIT).unwrap());
        drop(again);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_dropped_primary_frees_the_lock_while_a_copy_of_its_file_is_open() {
        // What a process being spawned holds until it execs.
        let dir = scratch("copy");
        let paths = Paths::at(&dir);
        let first = primary(claim(&paths, &Request::default(), ANSWER_WAIT).unwrap());
        let copy = first.lock.try_clone().unwrap();
        drop(first);
        let again =
            primary(claim(&paths, &Request::default(), Duration::from_millis(300)).unwrap());
        drop(again);
        drop(copy);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_holder_that_never_answers_is_an_error_and_its_socket_is_left_alone() {
        let dir = scratch("silent");
        let paths = Paths::at(&dir);
        fs::create_dir_all(&dir).unwrap();
        // Someone holds the lock but never listens.
        let held = File::create(paths.lock()).unwrap();
        held.try_lock().unwrap();
        fs::write(paths.info(), "4242\n").unwrap();
        #[cfg(unix)]
        fs::write(paths.socket(), "not a socket").unwrap();
        let started = Instant::now();
        let error = match claim(&paths, &Request::default(), Duration::from_millis(300)) {
            Err(e) => e,
            Ok(_) => panic!("nobody answered, so the claim must fail"),
        };
        assert!(started.elapsed() >= Duration::from_millis(300));
        assert!(
            matches!(error, Error::NoAnswer { pid: Some(4242) }),
            "{error}"
        );
        assert!(error.to_string().contains("--new-instance"));
        #[cfg(unix)]
        assert_eq!(
            fs::read_to_string(paths.socket()).unwrap(),
            "not a socket",
            "without the lock, the socket file must not be touched"
        );
        drop(held);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_stale_socket_file_is_replaced_under_the_lock() {
        let dir = scratch("stale");
        let paths = Paths::at(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(paths.socket(), "left by a killed app").unwrap();
        let first = primary(claim(&paths, &Request::default(), ANSWER_WAIT).unwrap());
        let (tx, rx) = mpsc::channel();
        first.serve(move |r| tx.send(r).unwrap()).unwrap();
        assert!(matches!(
            claim(&paths, &Request::default(), ANSWER_WAIT).unwrap(),
            Claim::Forwarded
        ));
        rx.recv_timeout(ANSWER_WAIT).unwrap();
        drop(first);
        assert!(
            !paths.socket().exists(),
            "dropping the primary removes its socket"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_long_folder_gets_a_short_private_socket_path() {
        let long = scratch("long").join("x".repeat(120));
        let paths = Paths::at(&long);
        let socket = paths.socket();
        assert!(
            socket.as_os_str().len() <= SOCKET_PATH_MAX,
            "{}",
            socket.display()
        );
        assert!(socket.starts_with("/tmp"));
        let other = Paths::at(long.with_file_name("y".repeat(120)));
        assert_ne!(socket, other.socket(), "each folder has its own socket");
        let first = primary(claim(&paths, &Request::default(), ANSWER_WAIT).unwrap());
        first.serve(|_| {}).unwrap();
        assert!(matches!(
            claim(&paths, &Request::default(), ANSWER_WAIT).unwrap(),
            Claim::Forwarded
        ));
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(socket.parent().unwrap())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "the short socket folder is private");
        drop(first);
        let _ = fs::remove_dir_all(socket.parent().unwrap());
        let _ = fs::remove_dir_all(long.parent().unwrap());
    }

    #[test]
    fn bad_requests_are_dropped() {
        let ok = Request {
            token: Some("t".into()),
            vault: None,
        };
        assert_eq!(Request::decode(&ok.encode("s"), "s"), Some(ok.clone()));
        assert_eq!(
            Request::decode(&ok.encode("s"), "other"),
            None,
            "wrong secret"
        );
        assert_eq!(Request::decode("not json", ""), None);
        assert_eq!(
            Request::decode(r#"{"v":2,"cmd":"show","secret":""}"#, ""),
            None
        );
        assert_eq!(
            Request::decode(r#"{"v":1,"cmd":"quit","secret":""}"#, ""),
            None
        );
        assert_eq!(
            Request::decode(r#"{"v":1,"cmd":"show","token":"","secret":""}"#, ""),
            Some(Request::default()),
            "an empty token is no token"
        );
    }

    /// Child half of the next test: hold the lock until killed.
    #[test]
    fn instance_child_probe() {
        let Ok(dir) = std::env::var("MUNINN_INSTANCE_PROBE_DIR") else {
            return;
        };
        let paths = Paths::at(dir);
        let first = primary(claim(&paths, &Request::default(), ANSWER_WAIT).unwrap());
        first.serve(|_| {}).unwrap();
        println!("holding");
        std::thread::sleep(Duration::from_secs(60));
    }

    #[test]
    fn another_process_holds_it_and_a_kill_frees_it_at_once() {
        let dir = scratch("kill");
        let paths = Paths::at(&dir);
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "instance::tests::instance_child_probe",
                "--nocapture",
            ])
            .env("MUNINN_INSTANCE_PROBE_DIR", &dir)
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut out = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        while !line.contains("holding") {
            line.clear();
            assert_ne!(
                out.read_line(&mut line).unwrap(),
                0,
                "the child exited early"
            );
        }
        assert!(matches!(
            claim(&paths, &Request::default(), ANSWER_WAIT).unwrap(),
            Claim::Forwarded
        ));
        // The hard kill of #42's "kill -9 relaunch": no stale lock remains.
        child.kill().unwrap();
        child.wait().unwrap();
        let again = primary(claim(&paths, &Request::default(), ANSWER_WAIT).unwrap());
        drop(again);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn paths_follow_the_runtime_dir() {
        let paths = Paths::at("/run/user/1000/muninn");
        assert_eq!(
            paths.lock(),
            Path::new("/run/user/1000/muninn/instance.lock")
        );
        #[cfg(unix)]
        assert_eq!(
            paths.socket(),
            Path::new("/run/user/1000/muninn/instance.sock")
        );
    }
}
