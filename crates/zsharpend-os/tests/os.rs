use zsharpend_os::{display_width, fnmatch, glob};

#[test]
fn matching() {
    assert!(fnmatch("*.rs", "lib.rs"));
    assert!(fnmatch("l?b.[a-r]s", "lib.rs"));
    assert!(!fnmatch("[!l]*", "lib.rs"));
    assert!(!fnmatch("*.c", "lib.rs"));
}

#[test]
fn globbing() {
    let dir = std::env::temp_dir().join(format!("zsharpend-os-glob-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("a/b")).unwrap();
    std::fs::write(dir.join("a/one.txt"), "").unwrap();
    std::fs::write(dir.join("a/b/two.txt"), "").unwrap();
    let d = dir.display();
    assert_eq!(glob(&format!("{d}/a/*.txt")), vec![format!("{d}/a/one.txt")]);
    assert_eq!(glob(&format!("{d}/**/*.txt")), vec![format!("{d}/a/b/two.txt"), format!("{d}/a/one.txt")]);
    assert!(glob(&format!("{d}/nothing*")).is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn widths() {
    assert_eq!(display_width("abc"), 3);
    assert_eq!(display_width("\x1b[1;31mred\x1b[0m"), 3);
    assert_eq!(display_width("日本"), 4);
}
