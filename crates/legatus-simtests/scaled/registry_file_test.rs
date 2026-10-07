//! Story 125 file tests on a real disk: unreadable, unparsable, loaded, and never written.
use legatus_proxy::config::read::read_registry;
use legatus_proxy::config::registry::ErrorCode;
use std::path::PathBuf;

const SPEC_EXAMPLE: &str = include_str!("../fixtures/registry/spec_example.yaml");

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("legatus-registry-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn tc02_a_missing_path_gives_file_unreadable_with_the_path_and_no_record() {
    let path = scratch("missing").join("absent.yaml");
    let errors = read_registry(&path).expect_err("no record");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, ErrorCode::FileUnreadable);
    assert_eq!(errors[0].path, path.display().to_string());
    assert_eq!(errors[0].text, "File does not exist.");
}

#[test]
fn tc02_a_directory_and_a_file_without_permission_are_file_unreadable() {
    let dir = scratch("dir");
    let errors = read_registry(&dir).expect_err("a directory is not a file");
    assert_eq!(errors[0].code, ErrorCode::FileUnreadable);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let file = dir.join("locked.yaml");
        std::fs::write(&file, "version: 1\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
        let readable_anyway = std::fs::read(&file).is_ok(); // running as root
        if !readable_anyway {
            let errors = read_registry(&file).expect_err("no permission");
            assert_eq!((errors[0].code, errors[0].text.as_str()), (ErrorCode::FileUnreadable, "File is not readable."));
        }
    }
}

#[test]
fn tc03_a_tab_indented_broken_file_is_file_unparsable_with_a_place_and_not_the_text() {
    let file = scratch("broken").join("broken.yaml");
    std::fs::write(&file, "version: 1\nnodes:\n\tn1: {bad: [unterminated\n").unwrap();
    let errors = read_registry(&file).expect_err("not valid text");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, ErrorCode::FileUnparsable);
    assert_eq!(errors[0].path, file.display().to_string());
    assert!(errors[0].text.starts_with("File is not valid structured text."), "{}", errors[0].text);
    assert!(!errors[0].text.contains("unterminated"));
}

#[test]
fn tc03_bytes_that_are_not_utf8_are_file_unparsable() {
    let file = scratch("binary").join("binary.yaml");
    std::fs::write(&file, [0xff, 0xfe, 0x00, 0x80]).unwrap();
    let errors = read_registry(&file).expect_err("not text");
    assert_eq!((errors[0].code, errors[0].text.as_str()), (ErrorCode::FileUnparsable, "File is not valid UTF-8 text."));
}

#[test]
fn tc01_the_spec_example_on_disk_loads_with_no_warnings() {
    let file = scratch("good").join("registry.yaml");
    std::fs::write(&file, SPEC_EXAMPLE).unwrap();
    let loaded = read_registry(&file).expect("loads");
    assert!(loaded.warnings.is_empty());
}

#[test]
fn tc16_a_successful_and_a_failed_load_leave_the_bytes_and_the_modification_time_unchanged() {
    let dir = scratch("untouched");
    for (name, text) in [("good.yaml", SPEC_EXAMPLE), ("bad.yaml", "version: 9\nnodes: {}\naliases: {}\nzzz: 1\n")] {
        let file = dir.join(name);
        std::fs::write(&file, text).unwrap();
        let before = std::fs::metadata(&file).unwrap().modified().unwrap();
        let _ = read_registry(&file);
        let _ = read_registry(&file);
        let after = std::fs::metadata(&file).unwrap().modified().unwrap();
        assert_eq!(before, after, "{name}: modification time");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), text, "{name}: bytes");
    }
}
