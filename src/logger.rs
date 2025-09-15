#[macro_export]
macro_rules! log_msg {
    (info, $($arg:tt)+) => {
        log::info!($($arg)+)
    };
    (error, $($arg:tt)+) => {
        log::error!($($arg)+)
    };
    (debug, $($arg:tt)+) => {
        if cfg!(debug_assertions) {
            log::debug!($($arg)+)
        }
    };
}
