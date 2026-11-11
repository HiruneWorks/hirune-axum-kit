use super::super::kubernetes_environment::Target;
use std::{fs, os::unix::fs::PermissionsExt, process::Command};

#[test]
fn maintenance_waits_for_all_gateways_without_exposing_error_pages() {
    if std::env::var_os("HIRUNE_GATEWAY_RECOVERY_CHILD").is_some() {
        let target = Target {
            context: "fixture".into(),
            namespace: "hirune-fixture".into(),
            kubeconfig: None,
        };
        super::set(&target, false).unwrap();
        return;
    }

    let temporary = tempfile::tempdir().unwrap();
    let directory = temporary.path();
    let kubectl = directory.join("kubectl");
    fs::write(&kubectl, include_str!("fixtures/gateway-kubectl.sh")).unwrap();
    fs::set_permissions(&kubectl, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(directory.join("maintenance"), "on").unwrap();

    // A child process keeps the kubectl fixture out of concurrent tests' PATH.
    let test_module = module_path!().split_once("::").unwrap().1;
    let test_name =
        format!("{test_module}::maintenance_waits_for_all_gateways_without_exposing_error_pages");
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &test_name, "--nocapture"])
        .env("PATH", directory)
        .env("HIRUNE_GATEWAY_RECOVERY_CHILD", "1")
        .env("HIRUNE_GATEWAY_FIXTURE_DIRECTORY", directory)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let events = fs::read_to_string(directory.join("events")).unwrap();
    let events: Vec<_> = events.lines().collect();
    let position = |event| events.iter().position(|value| *value == event).unwrap();
    assert!(position("ready:gateway-first") < position("delayed:on"));
    assert!(position("delayed:on") < position("ready:gateway-delayed"));
    assert!(position("ready:gateway-delayed") < position("maintenance:off"));
    assert_eq!(
        fs::read_to_string(directory.join("maintenance")).unwrap(),
        "off"
    );
    for stream in [output.stdout, output.stderr] {
        assert!(!String::from_utf8_lossy(&stream).contains("private-readiness-response"));
    }
}
