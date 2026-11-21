//! Code-level guard for CLAUDE.md safety rule 4: never weaken Microsoft Defender, the
//! firewall, security updates, UAC or SmartScreen.
//!
//! This is a deny-list, so it cannot be complete; every recipe still needs human review.
//! It exists so that an obvious mistake fails loudly at load time instead of shipping.
//! Matching is case-insensitive, like the Windows registry.

use super::model::{Action, RegistryAction};

/// Services the app must never reconfigure.
const PROTECTED_SERVICES: &[(&str, &str)] = &[
    ("WinDefend", "Microsoft Defender Antivirus"),
    ("WdNisSvc", "Microsoft Defender network inspection"),
    ("WdFilter", "Microsoft Defender filter driver"),
    ("WdBoot", "Microsoft Defender boot driver"),
    ("Sense", "Microsoft Defender for Endpoint"),
    ("SecurityHealthService", "Windows Security"),
    ("wscsvc", "Security Center"),
    ("mpssvc", "Windows Defender Firewall"),
    ("BFE", "Base Filtering Engine (firewall)"),
    ("wuauserv", "Windows Update"),
    ("UsoSvc", "Update Orchestrator"),
    ("WaaSMedicSvc", "Windows Update Medic"),
    (
        "BITS",
        "Background Intelligent Transfer (used by Windows Update)",
    ),
];

/// Registry key fragments that belong to Defender or the firewall.
const PROTECTED_KEY_FRAGMENTS: &[(&str, &str)] = &[
    (r"Microsoft\Windows Defender", "Microsoft Defender"),
    (
        r"Microsoft\Windows Advanced Threat Protection",
        "Microsoft Defender for Endpoint",
    ),
    (
        r"SharedAccess\Parameters\FirewallPolicy",
        "Windows Firewall",
    ),
    (r"Policies\Microsoft\WindowsFirewall", "Windows Firewall"),
    (r"SmartScreen", "SmartScreen"),
];

/// UAC values under `...\CurrentVersion\Policies\System`.
const UAC_KEY: &str = r"Microsoft\Windows\CurrentVersion\Policies\System";
const UAC_VALUES: &[&str] = &[
    "EnableLUA",
    "ConsentPromptBehaviorAdmin",
    "ConsentPromptBehaviorUser",
    "PromptOnSecureDesktop",
    "EnableInstallerDetection",
    "EnableSecureUIAPaths",
    "FilterAdministratorToken",
    "ValidateAdminCodeSignatures",
    "EnableVirtualization",
];

/// Windows Update policy values that stop or redirect security updates.
/// Deferring *feature* updates (e.g. `DeferFeatureUpdates`) stays allowed (SPEC 4.2).
const UPDATE_KEY: &str = r"Policies\Microsoft\Windows\WindowsUpdate";
const UPDATE_VALUES: &[&str] = &[
    "NoAutoUpdate",
    "AUOptions",
    "DisableWindowsUpdateAccess",
    "SetDisableUXWUAccess",
    "DoNotConnectToWindowsUpdateInternetLocations",
    "DeferQualityUpdates",
    "DeferQualityUpdatesPeriodInDays",
    "PauseQualityUpdatesStartTime",
    "WUServer",
    "WUStatusServer",
    "UseWUServer",
];

/// Preinstalled packages that Windows itself, its security or the Store depend on, plus
/// Edge (SPEC 4.2: Edge is only stopped in the background, never removed). Matched as a
/// case-insensitive part of the package name.
const PROTECTED_PACKAGES: &[(&str, &str)] = &[
    ("SecHealthUI", "Windows Security"),
    ("Windows.Apprep", "SmartScreen"),
    ("WindowsStore", "Microsoft Store"),
    ("StorePurchaseApp", "Microsoft Store"),
    ("DesktopAppInstaller", "App Installer (winget)"),
    ("VCLibs", "a shared app runtime"),
    ("UI.Xaml", "a shared app runtime"),
    ("NET.Native", "a shared app runtime"),
    ("WindowsAppRuntime", "a shared app runtime"),
    ("ShellExperienceHost", "the Windows shell"),
    ("StartMenuExperienceHost", "the Start menu"),
    ("CloudExperienceHost", "Windows sign-in and setup"),
    ("AAD.BrokerPlugin", "Microsoft account sign-in"),
    ("AccountsControl", "Microsoft account sign-in"),
    ("LockApp", "the lock screen"),
    ("MicrosoftEdge", "Microsoft Edge"),
    ("Windows.Search", "Windows Search"),
    ("XboxIdentityProvider", "Xbox sign-in used by games"),
    ("XboxGamingOverlay", "Game Bar (turned off, not removed)"),
    ("GamingApp", "the Xbox app used by Game Pass games"),
];

/// What a protected package is, if `package` is one.
pub fn is_protected_package(package: &str) -> Option<&'static str> {
    let lower = package.to_ascii_lowercase();
    PROTECTED_PACKAGES
        .iter()
        .find(|(part, _)| lower.contains(&part.to_ascii_lowercase()))
        .map(|(_, what)| *what)
}

/// Returns `Err((field, reason))` if the action touches a protected component.
pub(crate) fn check(action: &Action) -> Result<(), (String, String)> {
    match action {
        Action::Service(s) => match protected_service(&s.name) {
            Some(what) => Err((
                "name".into(),
                format!("`{}` is {what}; it is never changed", s.name),
            )),
            None => Ok(()),
        },
        Action::Registry(r) => check_registry(r),
        Action::Appx(a) => match is_protected_package(&a.package) {
            Some(what) => Err((
                "package".into(),
                format!("`{}` is {what}; it is never removed", a.package),
            )),
            None => Ok(()),
        },
        Action::PowerPlan(_)
        | Action::PowerSetting(_)
        | Action::NetAdapter(_)
        | Action::Cleanup(_) => Ok(()),
    }
}

fn protected_service(name: &str) -> Option<&'static str> {
    PROTECTED_SERVICES
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, what)| *what)
}

fn check_registry(r: &RegistryAction) -> Result<(), (String, String)> {
    let key = r.key.to_ascii_lowercase();
    let value = r.value.to_ascii_lowercase();
    let never = |what: &str| format!("{what} settings are never changed");

    for (fragment, what) in PROTECTED_KEY_FRAGMENTS {
        if key.contains(&fragment.to_ascii_lowercase()) {
            return Err(("key".into(), never(what)));
        }
    }
    if value.contains("smartscreen") {
        return Err(("value".into(), never("SmartScreen")));
    }

    // A service's own configuration key, e.g. SYSTEM\CurrentControlSet\Services\WinDefend.
    let mut segments = r.key.split('\\');
    while let Some(segment) = segments.next() {
        if segment.eq_ignore_ascii_case("Services") {
            if let Some(what) = segments.next().and_then(protected_service) {
                return Err(("key".into(), never(what)));
            }
        }
    }

    if key.ends_with(&UAC_KEY.to_ascii_lowercase())
        && UAC_VALUES.iter().any(|v| v.eq_ignore_ascii_case(&r.value))
    {
        return Err(("value".into(), never("User Account Control (UAC)")));
    }

    if key.contains(&UPDATE_KEY.to_ascii_lowercase())
        && UPDATE_VALUES
            .iter()
            .any(|v| v.eq_ignore_ascii_case(&r.value))
    {
        return Err(("value".into(), never("Security update")));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::model::{Hive, RegValue, RegistryOp, ServiceAction, ServiceStartup};

    fn reg(key: &str, value: &str) -> Action {
        Action::Registry(RegistryAction {
            hive: Hive::LocalMachine,
            key: key.into(),
            value: value.into(),
            op: RegistryOp::Set(RegValue::Dword(0)),
            only_if: None,
        })
    }

    fn svc(name: &str) -> Action {
        Action::Service(ServiceAction {
            name: name.into(),
            startup: ServiceStartup::Disabled,
        })
    }

    #[test]
    fn blocks_protected_services_case_insensitively() {
        for name in ["WinDefend", "windefend", "MpsSvc", "wuauserv", "BITS"] {
            assert!(check(&svc(name)).is_err(), "{name}");
        }
        assert!(check(&svc("Fax")).is_ok());
    }

    #[test]
    fn blocks_protected_registry_targets() {
        let blocked = [
            (
                r"SOFTWARE\Policies\Microsoft\Windows Defender",
                "DisableAntiSpyware",
            ),
            (
                r"SOFTWARE\Microsoft\Windows Defender\Real-Time Protection",
                "DisableRealtimeMonitoring",
            ),
            (
                r"SYSTEM\CurrentControlSet\Services\SharedAccess\Parameters\FirewallPolicy\StandardProfile",
                "EnableFirewall",
            ),
            (r"SYSTEM\CurrentControlSet\Services\WinDefend", "Start"),
            (r"SYSTEM\ControlSet001\Services\mpssvc", "Start"),
            (
                r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System",
                "EnableLUA",
            ),
            (
                r"SOFTWARE\Policies\Microsoft\Windows\System",
                "EnableSmartScreen",
            ),
            (
                r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer",
                "SmartScreenEnabled",
            ),
            (
                r"SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU",
                "NoAutoUpdate",
            ),
            (
                r"SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate",
                "WUServer",
            ),
        ];
        for (key, value) in blocked {
            assert!(check(&reg(key, value)).is_err(), "{key} / {value}");
        }
    }

    #[test]
    fn blocks_protected_packages() {
        for name in [
            "Microsoft.SecHealthUI",
            "Microsoft.WindowsStore",
            "microsoft.desktopappinstaller",
            "Microsoft.MicrosoftEdge.Stable",
            "Microsoft.VCLibs.140.00",
        ] {
            let action = Action::Appx(crate::recipe::AppxAction {
                package: name.into(),
            });
            assert!(check(&action).is_err(), "{name}");
        }
        assert!(is_protected_package("Clipchamp.Clipchamp").is_none());
        assert!(is_protected_package("Microsoft.BingNews").is_none());
    }

    #[test]
    fn allows_ordinary_tweaks() {
        let allowed = [
            (r"System\GameConfigStore", "GameDVR_Enabled"),
            (r"Software\Microsoft\GameBar", "AutoGameModeEnabled"),
            (
                r"SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate",
                "DeferFeatureUpdates",
            ),
            // Same value name as UAC, but a different key.
            (r"Software\PcOptimizerTest\Policies\Other", "EnableLUA"),
            (r"SYSTEM\CurrentControlSet\Services\Fax", "Start"),
        ];
        for (key, value) in allowed {
            assert!(check(&reg(key, value)).is_ok(), "{key} / {value}");
        }
    }
}
