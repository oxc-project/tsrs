#[cfg(unix)]
mod eventlist_test;
#[cfg(target_os = "macos")]
mod fsevents_darwin_test;
mod fsevents_load;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod isolated;
mod kqueue_macos;
#[cfg(target_os = "macos")]
mod kqueue_test;
#[cfg(unix)]
mod testutil_test;
#[cfg(target_os = "macos")]
mod watcher_consolidation_test;
#[cfg(unix)]
mod watcher_test;
mod write_settling;
mod write_settling_fsevents;
