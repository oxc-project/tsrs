use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::sync::Mutex;
use tsrs_vfs::vfs::FS;

use tsrs_fourslash_server::unix::*;

#[test]
fn file_payload_preserves_framing_and_missing_vs_empty() {
    let (client, peer) = UnixStream::pair().unwrap();
    let contents = "λ🦀\n\0\"\\".repeat(2048);
    let expected = contents.clone();
    let bridge = std::thread::spawn(move || {
        let mut peer = BufReader::new(peer);
        for payload in [None, Some(String::new()), Some(contents), Some("next file".into())] {
            let mut request = String::new();
            assert!(peer.read_line(&mut request).unwrap() > 0);
            match payload {
                None => writeln!(peer.get_mut(), "{{\"result\":null}}").unwrap(),
                Some(contents) => {
                    writeln!(peer.get_mut(), "{{\"result\":{}}}", contents.len()).unwrap();
                    peer.get_mut().write_all(contents.as_bytes()).unwrap();
                }
            }
        }
    });
    let fs = RemoteFS { stream: Mutex::new(BufReader::new(client)), case_sensitive: true };
    assert_eq!(fs.read_file("/missing.ts"), None);
    assert_eq!(fs.read_file("/empty.ts"), Some(String::new()));
    assert_eq!(fs.read_file("/unicode.ts"), Some(expected));
    assert_eq!(fs.read_file("/next.ts"), Some("next file".into()));
    bridge.join().unwrap();
}

#[test]
#[should_panic(expected = "read Go file contents")]
fn truncated_file_payload_fails() {
    let (client, mut peer) = UnixStream::pair().unwrap();
    peer.write_all(b"{\"result\":10}\nshort").unwrap();
    peer.shutdown(Shutdown::Write).unwrap();
    let fs = RemoteFS { stream: Mutex::new(BufReader::new(client)), case_sensitive: true };
    fs.read_file("/truncated.ts");
}
