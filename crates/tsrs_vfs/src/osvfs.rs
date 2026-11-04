pub mod os;
#[cfg(unix)]
pub mod sourcecache;

// The source cache needs Unix file identities (device, inode, status-change time); elsewhere it is never on.
#[cfg(not(unix))]
pub mod sourcecache {
    pub fn install(_key: &str) -> bool {
        false
    }

    pub fn report_counts(_enabled: bool) {}

    pub fn finish() {}
}

pub use self::os::*;
