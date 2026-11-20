//! One contract for every registry/service backend. The in-memory backend runs it on any OS;
//! the real Windows backend runs it only inside `HKCU\Software\PcOptimizerTest`
//! (CLAUDE.md safety rule 1).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use pco_core::backend::memory::{MemoryRegistry, MemoryServices};
use pco_core::backend::{
    BackendError, RegData, RegRead, RegistryBackend, ServiceBackend, ServiceStart, ServiceState,
};
use pco_core::recipe::{Hive, RegValue};

const HKCU: Hive = Hive::CurrentUser;

fn data(v: RegValue) -> RegData {
    RegData::from_value(&v)
}

/// The behaviour the engine relies on. `base` must not exist yet.
fn registry_contract(reg: &mut dyn RegistryBackend, base: &str) {
    let nested = format!(r"{base}\Nested");
    let deep = format!(r"{base}\Nested\Deep");

    assert_eq!(reg.read(HKCU, &deep, "Flag").unwrap(), RegRead::KeyMissing);

    // Writing creates the missing keys and reports them, outermost first.
    let created = reg
        .write(HKCU, &deep, "Flag", &data(RegValue::Dword(0)))
        .unwrap();
    assert!(
        created.ends_with(&[base.to_owned(), nested.clone(), deep.clone()]),
        "created: {created:?}"
    );

    // "Missing" and "0" are different states.
    assert_eq!(
        reg.read(HKCU, &deep, "Flag").unwrap(),
        RegRead::Present(data(RegValue::Dword(0)))
    );
    assert_eq!(
        reg.read(HKCU, &deep, "Other").unwrap(),
        RegRead::ValueMissing
    );

    // Value names are case-insensitive, like in Windows.
    assert!(matches!(
        reg.read(HKCU, &deep, "FLAG").unwrap(),
        RegRead::Present(_)
    ));

    // Overwriting creates nothing new.
    assert!(reg
        .write(HKCU, &deep, "Flag", &data(RegValue::Dword(1)))
        .unwrap()
        .is_empty());

    // Every supported type, and raw types a recipe never writes, round-trip exactly.
    let samples = [
        ("Text", data(RegValue::String("506".into()))),
        ("Diacritics", data(RegValue::String("ăîșțâ".into()))),
        ("Big", data(RegValue::Qword(u64::MAX))),
        (
            "Binary",
            RegData {
                reg_type: 3,
                bytes: vec![1, 2, 3],
            },
        ),
        (
            "Expand",
            RegData {
                reg_type: 2,
                bytes: data(RegValue::String("%TEMP%".into())).bytes,
            },
        ),
    ];
    for (name, d) in &samples {
        reg.write(HKCU, &deep, name, d).unwrap();
        assert_eq!(
            reg.read(HKCU, &deep, name).unwrap(),
            RegRead::Present(d.clone()),
            "{name}"
        );
    }

    // A key with values, or with subkeys, is never deleted.
    assert!(!reg.delete_key_if_empty(HKCU, &deep).unwrap());
    assert!(!reg.delete_key_if_empty(HKCU, base).unwrap());

    assert!(reg.delete_value(HKCU, &deep, "Flag").unwrap());
    assert!(!reg.delete_value(HKCU, &deep, "Flag").unwrap());
    assert_eq!(
        reg.read(HKCU, &deep, "Flag").unwrap(),
        RegRead::ValueMissing
    );
    for (name, _) in &samples {
        assert!(reg.delete_value(HKCU, &deep, name).unwrap());
    }

    // Remove exactly the keys that were created, innermost first.
    for key in [&deep, &nested, &base.to_owned()] {
        assert!(reg.delete_key_if_empty(HKCU, key).unwrap(), "{key}");
    }
    assert_eq!(reg.read(HKCU, &deep, "Flag").unwrap(), RegRead::KeyMissing);
    assert!(!reg.delete_value(HKCU, &deep, "Flag").unwrap());
    assert!(!reg.delete_key_if_empty(HKCU, &deep).unwrap());
}

#[test]
fn memory_registry_follows_the_contract() {
    let mut reg = MemoryRegistry::new().with_key(HKCU, r"Software\PcOptimizerTest");
    let before = reg.clone();
    registry_contract(&mut reg, r"Software\PcOptimizerTest\Contract");
    assert!(reg.same_contents(&before), "contract left something behind");
}

#[test]
fn memory_registry_injected_failures() {
    let key = r"Software\PcOptimizerTest\Fail";
    let mut reg = MemoryRegistry::new();
    reg.fail_on(HKCU, key, "Flag");
    let err = reg
        .write(HKCU, key, "flag", &data(RegValue::Dword(1)))
        .unwrap_err();
    assert!(matches!(err, BackendError::Injected { .. }));
    assert!(
        !reg.key_exists(HKCU, key),
        "a failed write must change nothing"
    );
    reg.write(HKCU, key, "Other", &data(RegValue::Dword(1)))
        .unwrap();
}

#[test]
fn memory_services() {
    let auto = ServiceState {
        start: ServiceStart::Automatic,
        delayed_auto_start: true,
    };
    let disabled = ServiceState {
        start: ServiceStart::Disabled,
        delayed_auto_start: true,
    };
    let mut svc = MemoryServices::new().with_service("Fax", auto);
    assert_eq!(svc.query("fax").unwrap(), Some(auto));
    assert_eq!(svc.query("NoSuchService").unwrap(), None);

    svc.set("Fax", disabled).unwrap();
    assert_eq!(svc.query("Fax").unwrap(), Some(disabled));
    assert!(matches!(
        svc.set("NoSuchService", disabled),
        Err(BackendError::ServiceNotFound { .. })
    ));

    svc.fail_on("Fax");
    assert!(matches!(
        svc.set("Fax", auto),
        Err(BackendError::Injected { .. })
    ));
    assert_eq!(svc.query("Fax").unwrap(), Some(disabled));
}

#[cfg(windows)]
mod real_windows {
    use super::*;
    use pco_core::backend::windows::{WindowsRegistry, WindowsServices, TEST_KEY};

    /// A fresh key below the test key, unique per test run.
    fn unique_base(test: &str) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!(r"{TEST_KEY}\{test}-{}-{nanos}", std::process::id())
    }

    #[test]
    fn windows_registry_follows_the_contract_in_the_test_key() {
        let mut reg = WindowsRegistry::test_sandbox();
        registry_contract(&mut reg, &unique_base("contract"));
    }

    #[test]
    fn sandbox_refuses_everything_outside_the_test_key() {
        let mut reg = WindowsRegistry::test_sandbox();
        let d = data(RegValue::Dword(0));
        let outside = [
            (HKCU, r"Software\Microsoft\GameBar"),
            (HKCU, r"Software\PcOptimizerTestX"),
            (Hive::LocalMachine, TEST_KEY),
        ];
        for (hive, key) in outside {
            assert!(
                matches!(
                    reg.write(hive, key, "AutoGameModeEnabled", &d),
                    Err(BackendError::OutsideSandbox { .. })
                ),
                "{key}"
            );
            assert!(matches!(
                reg.delete_value(hive, key, "AutoGameModeEnabled"),
                Err(BackendError::OutsideSandbox { .. })
            ));
            assert!(matches!(
                reg.delete_key_if_empty(hive, key),
                Err(BackendError::OutsideSandbox { .. })
            ));
        }
    }

    #[test]
    fn sandbox_still_reads_real_settings() {
        let reg = WindowsRegistry::test_sandbox();
        // Present on every Windows install.
        let read = reg
            .read(
                Hive::LocalMachine,
                r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
                "CurrentBuildNumber",
            )
            .unwrap();
        match read {
            RegRead::Present(d) => {
                let Some(RegValue::String(build)) = d.to_value() else {
                    panic!("unexpected data: {d:?}");
                };
                assert!(build.parse::<u32>().unwrap() > 0);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn services_are_read_only_in_tests() {
        let mut svc = WindowsServices::read_only();
        // The Event Log service exists on every Windows install.
        let state = svc.query("EventLog").unwrap().expect("EventLog exists");
        assert_eq!(state.start, ServiceStart::Automatic);
        assert_eq!(svc.query("PcoNoSuchService-2f1c").unwrap(), None);

        // A name that does not exist, so even a broken guard could not change anything.
        assert!(matches!(
            svc.set("PcoNoSuchService-2f1c", state),
            Err(BackendError::ReadOnly { .. })
        ));
    }
}
