use std::env;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::process::Command;

use tsrs_api_transport::transport::pipe_os::PipeListener;

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn private_listener_has_private_permissions_and_cleans_up() -> io::Result<()> {
    const HELPER_ENV: &str = "TSRS_PRIVATE_PIPE_PERMISSION_HELPER";
    if env::var(HELPER_ENV).as_deref() != Ok("1") {
        let output = Command::new("/bin/sh")
            .arg("-c")
            .arg(r#"umask 002; exec "$@""#)
            .arg("sh")
            .arg(env::current_exe()?)
            .arg("--exact")
            .arg("transport_unix_test::private_listener_has_private_permissions_and_cleans_up")
            .env(HELPER_ENV, "1")
            .output()?;
        assert!(
            output.status.success(),
            "permission helper failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Ok(());
    }

    let listener = PipeListener::bind_private("ignored-name")?;
    let socket_path = listener.path().to_path_buf();
    let directory = socket_path.parent().unwrap().to_path_buf();
    assert!(directory.file_name().unwrap().to_string_lossy().starts_with("tsgo-api-"));
    assert_eq!(mode(&directory), 0o700);
    assert_eq!(mode(&socket_path), 0o600);
    drop(listener);
    assert!(!directory.exists());

    let explicit = PipeListener::bind(&directory)?;
    assert_eq!(explicit.path(), directory.join("socket"));
    assert_eq!(mode(&directory), 0o700);
    assert_eq!(mode(explicit.path()), 0o600);
    drop(explicit);
    assert!(!directory.exists());
    Ok(())
}

#[test]
fn existing_directory_is_preserved() -> io::Result<()> {
    let temporary = PipeListener::bind_private("test")?;
    let directory = temporary.path().parent().unwrap().to_path_buf();
    let marker = directory.join("marker");
    fs::write(&marker, b"keep")?;
    let error = PipeListener::bind(&directory).err().unwrap();
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read(&marker)?, b"keep");
    fs::remove_file(marker)?;
    drop(temporary);
    Ok(())
}

#[test]
fn dropping_old_listener_preserves_replacement_socket() -> io::Result<()> {
    let old = PipeListener::bind_private("test")?;
    let socket_path = old.path().to_path_buf();
    let directory = socket_path.parent().unwrap().to_path_buf();
    fs::remove_file(&socket_path)?;
    let replacement = UnixListener::bind(&socket_path)?;
    drop(old);
    let _client = UnixStream::connect(&socket_path)?;
    drop(replacement);
    fs::remove_file(socket_path)?;
    fs::remove_dir(directory)?;
    Ok(())
}

#[test]
fn dropping_old_listener_preserves_replacement_directory() -> io::Result<()> {
    let old = PipeListener::bind_private("test")?;
    let directory = old.path().parent().unwrap().to_path_buf();
    let moved = directory.with_extension("moved");
    fs::rename(&directory, &moved)?;
    fs::create_dir(&directory)?;
    let replacement_path = directory.join("socket");
    let replacement = UnixListener::bind(&replacement_path)?;
    drop(old);
    let _client = UnixStream::connect(&replacement_path)?;
    drop(replacement);
    fs::remove_file(replacement_path)?;
    fs::remove_dir(directory)?;
    fs::remove_file(moved.join("socket"))?;
    fs::remove_dir(moved)?;
    Ok(())
}
