use std::fs;

use mev_revert_classifier::fetch::{sha256_hex, verify_seal, verify_seal_post};

fn setup(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mev-seal-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("datos")).unwrap();
    for (p, c) in [
        ("prereg-preregistro.md", "pre"),
        ("datos/muestra_bloques_v1.csv", "sample"),
        ("code.rs", "fn a(){}"),
    ] {
        fs::write(dir.join(p), c).unwrap();
    }
    let lines: Vec<String> = [
        "prereg-preregistro.md",
        "datos/muestra_bloques_v1.csv",
        "code.rs",
    ]
    .iter()
    .map(|p| format!("{}  {p}", sha256_hex(&fs::read(dir.join(p)).unwrap())))
    .collect();
    fs::write(
        dir.join("seal.sha256"),
        format!("# seal\n{}\n", lines.join("\n")),
    )
    .unwrap();
    dir
}

#[test]
fn post_seal_tolerates_code_changes_but_reports_them() {
    let d = setup("code");
    assert!(verify_seal_post(&d.join("seal.sha256"), &d)
        .unwrap()
        .is_empty());
    fs::write(d.join("code.rs"), "fn b(){}").unwrap();
    // Strict gate fails, post-seal gate lists the deviation.
    assert!(verify_seal(&d.join("seal.sha256"), &d).is_err());
    let changed = verify_seal_post(&d.join("seal.sha256"), &d).unwrap();
    assert_eq!(changed, vec!["code.rs".to_string()]);
    fs::remove_dir_all(&d).unwrap();
}

#[test]
fn post_seal_refuses_changed_preregistro_or_sample() {
    for p in ["prereg-preregistro.md", "datos/muestra_bloques_v1.csv"] {
        let d = setup("crit");
        fs::write(d.join(p), "tampered").unwrap();
        assert!(verify_seal_post(&d.join("seal.sha256"), &d).is_err(), "{p}");
        fs::remove_dir_all(&d).unwrap();
    }
    let d = setup("gone");
    fs::remove_file(d.join("datos/muestra_bloques_v1.csv")).unwrap();
    assert!(verify_seal_post(&d.join("seal.sha256"), &d).is_err());
    fs::remove_dir_all(&d).unwrap();
}
