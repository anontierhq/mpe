use std::time::Duration;

/// Runs `f` up to `1 + extra_retries` times. After each failure except the last, calls `on_retry`
/// then sleeps `retry_delay`.
pub(crate) fn run_with_retries<E>(
    extra_retries: u32,
    retry_delay: Duration,
    mut f: impl FnMut() -> Result<(), E>,
    mut on_retry: impl FnMut(u32, &E),
) -> Result<(), E> {
    let total = extra_retries.saturating_add(1).max(1);
    let mut last_err = None;
    for attempt in 1..=total {
        match f() {
            Ok(()) => return Ok(()),
            Err(e) => {
                if attempt < total {
                    on_retry(attempt, &e);
                    std::thread::sleep(retry_delay);
                }
                last_err = Some(e);
            }
        }
    }
    Err(last_err.expect("total >= 1 implies at least one attempt"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn succeeds_on_first_attempt() {
        let calls = AtomicU32::new(0);
        let r = run_with_retries(3, Duration::ZERO, || {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok::<(), ()>(())
        }, |_, _| {});
        assert!(r.is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn succeeds_after_failures() {
        let calls = AtomicU32::new(0);
        let r = run_with_retries(3, Duration::ZERO, || {
            let n = calls.fetch_add(1, Ordering::SeqCst);
            if n < 2 {
                Err(())
            } else {
                Ok(())
            }
        }, |_, _| {});
        assert!(r.is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn returns_last_error_when_exhausted() {
        let calls = AtomicU32::new(0);
        let r = run_with_retries(2, Duration::ZERO, || {
            let n = calls.fetch_add(1, Ordering::SeqCst);
            Err(n)
        }, |_, _| {});
        // fetch_add returns the previous value; third attempt returns Err(2).
        assert_eq!(r, Err(2));
    }

    #[test]
    fn on_retry_called_before_each_retry() {
        let calls = AtomicU32::new(0);
        let retries = AtomicU32::new(0);
        let _ = run_with_retries(2, Duration::ZERO, || {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(())
        }, |_, _| {
            retries.fetch_add(1, Ordering::SeqCst);
        });
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        assert_eq!(retries.load(Ordering::SeqCst), 2);
    }
}
