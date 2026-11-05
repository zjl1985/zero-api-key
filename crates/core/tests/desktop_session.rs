use std::process::Command;
use zero_api_key_lib::{
    config::Config,
    service::{CommitEntry, EntryInput, Service},
};

#[test]
fn isolated_desktop_and_cli_flow() {
    let directory = tempfile::tempdir().unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "desktop_flow_child", "--nocapture"])
        .env("ZERO_API_KEY_DATA_DIR", directory.path())
        .env("ZERO_API_KEY_TEST_CHILD", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn desktop_flow_child() {
    if std::env::var_os("ZERO_API_KEY_TEST_CHILD").is_none() {
        return;
    }
    let mut session = Service::new().unwrap();
    assert!(!session.is_locked());
    session.create_group("local", "sample").unwrap();
    let input = EntryInput {
        key: "api key".into(),
        value: "only-a-fixture's-value".into(),
        entry_type: "api-key".into(),
        env_name: Some("EXAMPLE_TOKEN".into()),
    };
    session.save_entry("local", "sample", None, input).unwrap();
    let catalog = session.catalog("local").unwrap();
    assert_eq!(catalog[0].entries[0].key, "API_KEY");
    assert_ne!(catalog[0].entries[0].masked_value, "only-a-fixture's-value");
    assert_eq!(
        session.reveal("local", "sample", "API_KEY").unwrap(),
        "only-a-fixture's-value"
    );
    assert!(
        session
            .export("local", "sample")
            .unwrap()
            .contains("export EXAMPLE_TOKEN=")
    );

    let output = Command::new(env!("CARGO_BIN_EXE_zak"))
        .args(["get", "EXAMPLE_TOKEN", "--local", "--reveal", "--raw"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "only-a-fixture's-value"
    );

    let path = Config::path().unwrap().with_file_name("import.ini");
    std::fs::write(&path, "[second]\nAPI_KEY=another-fixture\n").unwrap();
    let preview = session.import_preview(path.to_str().unwrap()).unwrap();
    assert_eq!(preview.len(), 1);
    assert_eq!(
        session
            .import_commit(
                "local",
                vec![CommitEntry {
                    group: "second".into(),
                    key: preview[0].key.clone(),
                    value: preview[0].value.clone(),
                    entry_type: preview[0].entry_type.clone()
                }]
            )
            .unwrap(),
        1
    );
    session
        .save_entry(
            "local",
            "second",
            Some("API_KEY"),
            EntryInput {
                key: "TOKEN".into(),
                value: "changed".into(),
                entry_type: "note".into(),
                env_name: None,
            },
        )
        .unwrap();
    assert_eq!(
        session.reveal("local", "second", "TOKEN").unwrap(),
        "changed"
    );
    session.remove("local", "second", Some("TOKEN")).unwrap();
    session.remove("local", "second", None).unwrap();

    session
        .set_password(None, Some("fixture-password"))
        .unwrap();
    session.lock();
    assert!(session.catalog("local").is_err());
    assert!(session.unlock("wrong").is_err());
    assert!(session.is_locked());
    session.unlock("fixture-password").unwrap();
    assert_eq!(session.catalog("local").unwrap().len(), 1);
    assert!(Service::new().unwrap().is_locked());
    session
        .set_password(Some("fixture-password"), None)
        .unwrap();
    session.set_touch_id(true).unwrap();
    assert!(Service::new().unwrap().is_locked());
    session.lock();
    assert!(session.unlock("").is_err());

    // Invalid configuration never silently opens an unlocked session.
    std::fs::write(Config::path().unwrap(), "this is not valid TOML = [").unwrap();
    assert!(Service::new().is_err());
}
