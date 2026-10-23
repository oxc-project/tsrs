use std::fmt;
use std::sync::{Mutex, Weak};

use tsrs_lsproto as lsproto;
use tsrs_lsproto::{LogVerbosity, MessageType};

use crate::server::Server;

// logger.go:13. Go's nil-receiver checks (`if l == nil`) have no counterpart: the server always has a logger.
pub struct LoggerImpl {
    server: Weak<Server>,
    mu: Mutex<LogVerbosity>,
}

// logger.go:19
pub fn new_logger(server: Weak<Server>) -> LoggerImpl {
    LoggerImpl { server, mu: Mutex::new(LogVerbosity::INFO) }
}

// logger.go:28
// maxVerbosityForMessageType returns the least-verbose log level at which
// messages of the given LSP MessageType should still be sent.
fn max_verbosity_for_message_type(msg_type: MessageType) -> LogVerbosity {
    match msg_type {
        MessageType::ERROR => LogVerbosity::ERROR,
        MessageType::WARNING => LogVerbosity::WARNING,
        MessageType::DEBUG => LogVerbosity::DEBUG,
        _ => LogVerbosity::INFO,
    }
}

// logger.go:44
// isValidLogVerbosity reports whether v is one of the defined LogVerbosity values.
pub fn is_valid_log_verbosity(v: LogVerbosity) -> bool {
    v.0 >= LogVerbosity::OFF.0 && v.0 <= LogVerbosity::ERROR.0
}

impl LoggerImpl {
    // logger.go:48
    fn send_log_message(&self, msg_type: MessageType, message: &str) {
        let Some(server) = self.server.upgrade() else {
            return;
        };

        if !server.init_started.load(std::sync::atomic::Ordering::SeqCst) {
            server.write_stderr_line(message);
            return;
        }

        // Don't send messages that the client will filter out anyway.
        let verbosity = *self.mu.lock().unwrap();
        if verbosity == LogVerbosity::OFF
            || verbosity.0 > max_verbosity_for_message_type(msg_type).0
        {
            return;
        }

        let notification =
            lsproto::WINDOW_LOG_MESSAGE_INFO.new_notification_message(lsproto::LogMessageParams {
                type_: msg_type,
                message: message.to_string(),
            });

        let background_ctx = server.background_ctx();
        if server.outgoing_queue.put(&background_ctx, notification.message()).is_err()
            && background_ctx.err().is_some()
        {
            server.write_stderr_line(message);
        }
    }

    // logger.go:78
    pub fn log(&self, msg: &str) {
        self.send_log_message(MessageType::INFO, msg);
    }

    // logger.go:85
    pub fn logf(&self, args: fmt::Arguments<'_>) {
        self.send_log_message(MessageType::INFO, &fmt::format(args));
    }

    // logger.go:92
    pub fn verbose(&self) -> Option<&LoggerImpl> {
        let verbosity = *self.mu.lock().unwrap();
        if verbosity == LogVerbosity::OFF || verbosity.0 > LogVerbosity::DEBUG.0 {
            return None;
        }
        Some(self)
    }

    // logger.go:104
    pub fn is_verbose(&self) -> bool {
        let verbosity = *self.mu.lock().unwrap();
        verbosity.0 >= LogVerbosity::TRACE.0 && verbosity.0 <= LogVerbosity::DEBUG.0
    }

    // logger.go:113
    pub fn set_verbose(&self, verbose: bool) {
        let mut verbosity = self.mu.lock().unwrap();
        if verbose {
            *verbosity = LogVerbosity::DEBUG;
        } else {
            *verbosity = LogVerbosity::INFO;
        }
    }

    // logger.go:126
    pub fn is_tracing(&self) -> bool {
        *self.mu.lock().unwrap() == LogVerbosity::TRACE
    }

    // logger.go:135
    pub fn set_verbosity(&self, verbosity: LogVerbosity) {
        *self.mu.lock().unwrap() = verbosity;
    }

    // logger.go:144
    pub fn error(&self, msg: &str) {
        self.send_log_message(MessageType::ERROR, msg);
    }

    // logger.go:151
    pub fn errorf(&self, args: fmt::Arguments<'_>) {
        self.send_log_message(MessageType::ERROR, &fmt::format(args));
    }

    // logger.go:158
    pub fn warn(&self, msg: &str) {
        self.send_log_message(MessageType::WARNING, msg);
    }

    // logger.go:165
    pub fn warnf(&self, args: fmt::Arguments<'_>) {
        self.send_log_message(MessageType::WARNING, &fmt::format(args));
    }

    // logger.go:172
    pub fn info(&self, msg: &str) {
        self.send_log_message(MessageType::INFO, msg);
    }

    // logger.go:179
    pub fn infof(&self, args: fmt::Arguments<'_>) {
        self.send_log_message(MessageType::INFO, &fmt::format(args));
    }
}

// logger.go:11: `_ logging.Logger = (*logger)(nil)`.
impl tsrs_project::logging::Logger for LoggerImpl {
    fn error(&self, msg: &str) {
        LoggerImpl::error(self, msg);
    }
    fn errorf(&self, args: fmt::Arguments<'_>) {
        LoggerImpl::errorf(self, args);
    }
    fn warn(&self, msg: &str) {
        LoggerImpl::warn(self, msg);
    }
    fn warnf(&self, args: fmt::Arguments<'_>) {
        LoggerImpl::warnf(self, args);
    }
    fn info(&self, msg: &str) {
        LoggerImpl::info(self, msg);
    }
    fn infof(&self, args: fmt::Arguments<'_>) {
        LoggerImpl::infof(self, args);
    }
    fn log(&self, msg: &str) {
        LoggerImpl::log(self, msg);
    }
    fn logf(&self, args: fmt::Arguments<'_>) {
        LoggerImpl::logf(self, args);
    }
    fn verbose(&self) -> Option<&dyn tsrs_project::logging::Logger> {
        LoggerImpl::verbose(self).map(|l| l as &dyn tsrs_project::logging::Logger)
    }
    fn is_verbose(&self) -> bool {
        LoggerImpl::is_verbose(self)
    }
    fn set_verbose(&self, verbose: bool) {
        LoggerImpl::set_verbose(self, verbose);
    }
}
