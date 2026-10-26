// Port of Go `internal/fswatch` (itself a Go port of @parcel/watcher): in-process filesystem watching with
// FSEvents and kqueue (macOS / BSD) and inotify (Linux). Not ported: `fanotify_linux.go` (Linux uses inotify, the
// same fallback Go takes on kernels without fanotify), `windows.go` (Windows builds get an unavailable watcher),
// `walkDirGeneric` (only used where the unix walker does not exist).

pub mod canonicalize;
pub mod debounce;
pub mod event;
#[cfg(target_os = "macos")]
pub mod fsevents_darwin;
#[cfg(target_os = "macos")]
pub mod fsevents_darwin_ffi;
#[cfg(target_os = "linux")]
mod inotify_linux;
#[cfg(any(
    target_os = "macos",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly"
))]
pub mod kqueue;
pub mod pathcompare;
pub mod pathkey;
#[cfg(unix)]
mod walkdir_unix;
pub mod watcher;

pub use canonicalize::path_comparer_for_path;
pub use event::{Event, EventKind};
pub use pathkey::{NATIVE_PATH_COMPARISON_AVAILABLE, PathComparer};
pub use watcher::{
    ERR_FILESYSTEM_UNSUPPORTED, ERR_OVERFLOW, ERR_UNAVAILABLE, ERR_WATCH_TERMINATED, Error,
    IgnoreFunc, Sentinel, Watch, WatchCallback, WatchDirectoryRequest, WatchOption, Watcher,
    all_watchers, default, fanotify, fsevents, inotify, kqueue, set_write_settling, windows,
    with_ignore, with_recursive,
};
