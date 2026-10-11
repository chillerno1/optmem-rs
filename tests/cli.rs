use std::process::Command;

fn memo(dir: &std::path::Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_memo"))
        .args(args)
        .env("MEMORY_DIR", dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn init_note_wake_recall() {
    let dir = std::env::temp_dir().join(format!("optmem-rs-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    memo(&dir, &["init"]);
    assert_eq!(
        memo(&dir, &["note", "the build passes on stable"]),
        "Saved as #0.\n"
    );
    let wake = memo(&dir, &["wake"]);
    assert!(wake.contains("the build passes on stable"), "{wake}");
    assert!(wake.ends_with("You are awake.\n"), "{wake}");
    assert!(memo(&dir, &["recall", "STABLE"]).ends_with("1 match.\n"));
    std::fs::remove_dir_all(&dir).unwrap();
}
