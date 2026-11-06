// tsrs-only: what the client and the server of a resident check server agree on (daemon/mod.rs): which server a
// command belongs to (`key_material`), where its socket is (`Paths`), and the two messages they exchange.

use std::fmt::Write as _;
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::PathBuf;

/// Changes whenever the messages or the meaning of the key change.
const PROTOCOL: &str = "tsrs-daemon 1";
const MAGIC: [u8; 4] = *b"TSRD";

/// Everything a cold run's output depends on besides the file system: the protocol, this binary (its path, file
/// identity and version), the working directory, the arguments and the environment variables tsrs reads. Two
/// commands with the same material are served by the same server; the server compares the whole text, not its hash.
/// `None` when the binary or the working directory cannot be found.
pub fn key_material(args: &[String]) -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let md = std::fs::metadata(&exe).ok()?;
    let cwd = std::env::current_dir().ok()?;
    let mut text = format!(
        "{PROTOCOL}\n{} {}\n{} {} {} {} {}.{}\n{}\n",
        env!("TSRS_RELEASE_VERSION"),
        env!("TSRS_TYPESCRIPT_COMMIT"),
        exe.display(),
        md.dev(),
        md.ino(),
        md.size(),
        md.mtime(),
        md.mtime_nsec(),
        cwd.display(),
    );
    for arg in args {
        text.push_str("arg ");
        text.push_str(&arg.replace('\n', "\\n"));
        text.push('\n');
    }
    let mut env: Vec<(String, String)> = std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
        .filter(|(k, _)| relevant_env(k))
        .collect();
    env.sort();
    for (k, v) in env {
        let _ = writeln!(text, "env {k}={}", v.replace('\n', "\\n"));
    }
    Some(text)
}

/// The variables tsrs reads (its own, `TS_*` debug switches, the allocator's and the worker pool's, and the colour
/// switches of the diagnostic reporter), except the daemon's own settings.
fn relevant_env(name: &str) -> bool {
    (name.starts_with("TSRS_") && !name.starts_with("TSRS_DAEMON"))
        || name.starts_with("TS_")
        || name.starts_with("MIMALLOC_")
        || name.starts_with("RAYON_")
        || matches!(name, "TERM" | "NO_COLOR" | "FORCE_COLOR")
}

/// The files of the server for a key, in a directory only this user can enter.
pub struct Paths {
    pub socket: PathBuf,
    pub lock: PathBuf,
    pub log: PathBuf,
}

/// `TSRS_DAEMON_DIR`, else `$XDG_RUNTIME_DIR/tsrs-daemon`, else `$TMPDIR/tsrs-daemon-<uid>` (`/tmp` without one).
/// Created with mode 0700; `None` when it exists and is not a directory of this user's that only they can use.
pub fn directory() -> Option<PathBuf> {
    // SAFETY: getuid has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    let dir = if let Some(dir) = std::env::var_os("TSRS_DAEMON_DIR").filter(|d| !d.is_empty()) {
        PathBuf::from(dir)
    } else if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty()) {
        PathBuf::from(runtime).join("tsrs-daemon")
    } else {
        std::env::temp_dir().join(format!("tsrs-daemon-{uid}"))
    };
    let _ = std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir);
    let md = std::fs::symlink_metadata(&dir).ok()?;
    (md.is_dir() && md.uid() == uid && md.permissions().mode().trailing_zeros() >= 6).then_some(dir)
}

impl Paths {
    pub fn new(material: &str) -> Option<Paths> {
        let dir = directory()?;
        let name = format!("{:016x}", xxhash_rust::xxh3::xxh3_64(material.as_bytes()));
        Some(Paths {
            socket: dir.join(format!("{name}.sock")),
            lock: dir.join(format!("{name}.lock")),
            log: dir.join(format!("{name}.log")),
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    Check = 0,
    Stop = 1,
}

pub struct Request {
    pub material: String,
    pub op: Op,
    pub stdout_is_tty: bool,
}

pub enum Response {
    /// The run's standard output and exit status.
    Output { status: u8, stdout: Vec<u8> },
    /// The client runs the command itself.
    Fallback,
    /// The server stopped; the client starts a new one.
    Restart,
    /// Answer to `Op::Stop`.
    Stopped,
}

fn read_exact<const N: usize>(r: &mut impl Read) -> std::io::Result<[u8; N]> {
    let mut buf = [0; N];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

fn invalid(what: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, what.to_string())
}

pub fn write_request(w: &mut impl Write, request: &Request) -> std::io::Result<()> {
    let mut buf = MAGIC.to_vec();
    let len = u32::try_from(request.material.len()).map_err(|_| invalid("key"))?;
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(request.material.as_bytes());
    buf.push(request.op as u8);
    buf.push(u8::from(request.stdout_is_tty));
    w.write_all(&buf)?;
    w.flush()
}

pub fn read_request(r: &mut impl Read) -> std::io::Result<Request> {
    if read_exact::<4>(r)? != MAGIC {
        return Err(invalid("magic"));
    }
    let len = u32::from_le_bytes(read_exact::<4>(r)?) as usize;
    if len > 1 << 20 {
        return Err(invalid("key length"));
    }
    let mut material = vec![0; len];
    r.read_exact(&mut material)?;
    let [op, tty] = read_exact::<2>(r)?;
    Ok(Request {
        material: String::from_utf8(material).map_err(|_| invalid("key"))?,
        op: match op {
            0 => Op::Check,
            1 => Op::Stop,
            _ => return Err(invalid("op")),
        },
        stdout_is_tty: tty != 0,
    })
}

pub fn write_response(w: &mut impl Write, response: &Response) -> std::io::Result<()> {
    let mut head = MAGIC.to_vec();
    match response {
        Response::Output { status, stdout } => {
            head.extend_from_slice(&[0, *status]);
            head.extend_from_slice(&(stdout.len() as u64).to_le_bytes());
            w.write_all(&head)?;
            w.write_all(stdout)?;
        }
        Response::Fallback => {
            head.push(1);
            w.write_all(&head)?;
        }
        Response::Restart => {
            head.push(2);
            w.write_all(&head)?;
        }
        Response::Stopped => {
            head.push(3);
            w.write_all(&head)?;
        }
    }
    w.flush()
}

pub fn read_response(r: &mut impl Read) -> std::io::Result<Response> {
    if read_exact::<4>(r)? != MAGIC {
        return Err(invalid("magic"));
    }
    Ok(match read_exact::<1>(r)?[0] {
        0 => {
            let status = read_exact::<1>(r)?[0];
            let len = usize::try_from(u64::from_le_bytes(read_exact::<8>(r)?))
                .map_err(|_| invalid("length"))?;
            let mut stdout = vec![0; len];
            r.read_exact(&mut stdout)?;
            Response::Output { status, stdout }
        }
        1 => Response::Fallback,
        2 => Response::Restart,
        3 => Response::Stopped,
        _ => return Err(invalid("response")),
    })
}
