use std::ffi::c_void;
use std::io;
use std::io::Read;
use std::io::Write;
use std::os::windows::io::AsRawHandle;
use std::ptr;
use windows_sys::Win32::Security::Authorization::SDDL_REVISION_1;

use std::fs::OpenOptions;
use tsrs_api_transport::transport::pipe_os::*;

use windows_sys::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW, GetSecurityInfo, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::DACL_SECURITY_INFORMATION;

fn dacl_sddl(descriptor: *mut c_void) -> String {
    let mut raw_sddl = ptr::null_mut();
    // SAFETY: `descriptor` is a live security descriptor and `raw_sddl` is a writable
    // out-parameter. The returned string is freed with LocalFree below.
    assert_ne!(
        unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor,
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &mut raw_sddl,
                ptr::null_mut(),
            )
        },
        0,
        "{}",
        io::Error::last_os_error()
    );
    let _allocation = LocalAllocation(raw_sddl.cast());
    let mut units = Vec::new();
    let mut cursor = raw_sddl;
    // SAFETY: the conversion returned a NUL-terminated UTF-16 string held by `_allocation`.
    unsafe {
        while *cursor != 0 {
            units.push(*cursor);
            cursor = cursor.add(1);
        }
    }
    String::from_utf16(&units).expect("Windows returned valid UTF-16 SDDL")
}

#[test]
fn private_pipe_accepts_current_user_with_one_protected_ace() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is after epoch")
        .as_nanos();
    let listener = PipeListener::bind_private(&format!("tsrs-api-{nonce}-{}", std::process::id()))
        .expect("bind private pipe");
    assert!(listener.path().to_string_lossy().starts_with(r"\\.\pipe\"));
    assert!(PipeListener::bind(listener.path()).is_err(), "the pipe name cannot be rebound");

    let actual = {
        let guard = listener.pipe.lock().expect("listener lock");
        let pipe = guard.as_ref().expect("listener owns first instance");
        let mut descriptor = ptr::null_mut();
        // SAFETY: `pipe` owns a live named-pipe handle, and `descriptor` is a writable
        // out-parameter. GetSecurityInfo allocates a self-relative descriptor.
        let status = unsafe {
            GetSecurityInfo(
                pipe.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                &mut descriptor,
            )
        };
        assert_eq!(status, 0, "{}", io::Error::from_raw_os_error(status as i32));
        let _allocation = LocalAllocation(descriptor);
        dacl_sddl(descriptor)
    };
    let expected_descriptor = private_security_descriptor().expect("current-user DACL");
    let expected = dacl_sddl(expected_descriptor.0);
    // Windows can expand generic-all access to file-all access on a named-pipe handle.
    let normalize_access = |sddl: &str| sddl.replace(";;FA;;;", ";;GA;;;");
    assert_eq!(
        normalize_access(&actual),
        normalize_access(&expected),
        "pipe DACL: {actual}; generated DACL: {expected}"
    );
    assert!(actual.starts_with("D:P("), "DACL must be protected: {actual}");
    assert_eq!(actual.matches('(').count(), 1, "expected one access-control entry: {actual}");

    let path = listener.path().to_path_buf();
    let server = std::thread::spawn(move || {
        let mut stream = listener.accept().expect("accept current-user client");
        let mut request = [0; 4];
        stream.reader.read_exact(&mut request).expect("read request");
        assert_eq!(&request, b"ping");
        stream.writer.write_all(b"pong").expect("write response");
    });
    let mut client =
        OpenOptions::new().read(true).write(true).open(path).expect("current user can connect");
    client.write_all(b"ping").expect("write request");
    let mut response = [0; 4];
    client.read_exact(&mut response).expect("read response");
    assert_eq!(&response, b"pong");
    server.join().expect("server thread");
}
