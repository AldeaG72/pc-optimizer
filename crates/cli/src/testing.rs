//! Test aid for the "interrupted in the middle" check (ETAPA-0, exit criterion 2).
//!
//! With `PCO_TEST_STOP_AFTER_CHANGES=N`, `pco` stops abruptly right after its N-th registry
//! change — no cleanup, no journal update — exactly like a crash or a power cut. The next
//! `pco` command must then undo the interrupted session. It only ever makes `pco` stop
//! earlier; it cannot make it change anything it would not change anyway.

use pco_core::backend::{BackendError, RegData, RegRead, RegistryBackend};
use pco_core::recipe::Hive;

pub const STOP_AFTER_CHANGES_VAR: &str = "PCO_TEST_STOP_AFTER_CHANGES";

/// Exit code used when stopping on purpose.
pub const STOP_EXIT_CODE: i32 = 99;

pub struct StopAfterChanges<R> {
    inner: R,
    changes_left: usize,
    stop: Box<dyn Fn()>,
}

impl<R: RegistryBackend> StopAfterChanges<R> {
    /// Stops the process (exit code 99) after `changes` successful changes.
    pub fn new(inner: R, changes: usize) -> Self {
        Self::with_stop(
            inner,
            changes,
            Box::new(|| {
                eprintln!("{STOP_AFTER_CHANGES_VAR}: stopping now, on purpose (test)");
                std::process::exit(STOP_EXIT_CODE);
            }),
        )
    }

    /// Same, with a custom stop action (tests).
    pub fn with_stop(inner: R, changes: usize, stop: Box<dyn Fn()>) -> Self {
        Self {
            inner,
            changes_left: changes,
            stop,
        }
    }

    fn changed(&mut self) {
        self.changes_left = self.changes_left.saturating_sub(1);
        if self.changes_left == 0 {
            (self.stop)();
        }
    }
}

impl<R: RegistryBackend> RegistryBackend for StopAfterChanges<R> {
    fn read(&self, hive: Hive, key: &str, value: &str) -> Result<RegRead, BackendError> {
        self.inner.read(hive, key, value)
    }

    fn write(
        &mut self,
        hive: Hive,
        key: &str,
        value: &str,
        data: &RegData,
    ) -> Result<Vec<String>, BackendError> {
        let created = self.inner.write(hive, key, value, data)?;
        self.changed();
        Ok(created)
    }

    fn delete_value(&mut self, hive: Hive, key: &str, value: &str) -> Result<bool, BackendError> {
        let deleted = self.inner.delete_value(hive, key, value)?;
        if deleted {
            self.changed();
        }
        Ok(deleted)
    }

    fn delete_key_if_empty(&mut self, hive: Hive, key: &str) -> Result<bool, BackendError> {
        self.inner.delete_key_if_empty(hive, key)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use pco_core::backend::memory::MemoryRegistry;
    use pco_core::recipe::RegValue;

    use super::*;

    #[test]
    fn stops_exactly_after_the_nth_change() {
        let stops = Rc::new(Cell::new(0));
        let counter = Rc::clone(&stops);
        let mut reg = StopAfterChanges::with_stop(
            MemoryRegistry::new(),
            2,
            Box::new(move || counter.set(counter.get() + 1)),
        );
        let d = RegData::from_value(&RegValue::Dword(1));
        let key = r"Software\PcOptimizerTest";

        reg.write(Hive::CurrentUser, key, "A", &d).unwrap();
        assert_eq!(stops.get(), 0);
        // Deleting something that is not there is not a change.
        reg.delete_value(Hive::CurrentUser, key, "Missing").unwrap();
        assert_eq!(stops.get(), 0);
        reg.write(Hive::CurrentUser, key, "B", &d).unwrap();
        assert_eq!(stops.get(), 1, "stops right after the second change");
        // Reads never count.
        reg.read(Hive::CurrentUser, key, "A").unwrap();
        assert_eq!(stops.get(), 1);
    }
}
