use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

#[test]
fn numbered_provider_tests_match_numbered_fixtures() {
    let manifest_directory = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_directory = manifest_directory
        .parent()
        .and_then(Path::parent)
        .expect("provider crate should live under workspace crates directory");
    let fixtures_directory = workspace_directory.join("crates").join("superwire-test-support").join("fixtures");
    let tests_directory = manifest_directory.join("tests");

    let fixture_names = numbered_file_stems(&fixtures_directory, ".wire");
    let test_names = numbered_file_stems(&tests_directory, "_test.rs");

    // Hard rule: numbered provider fixtures and tests are paired by exact stem.
    // `044_name.wire` must be covered by `044_name_test.rs`; do not cross-link numbered fixtures.
    assert_eq!(fixture_names, test_names);
}

fn numbered_file_stems(directory: &Path, suffix: &str) -> BTreeSet<String> {
    fs::read_dir(directory)
        .expect("fixture pair directory should be readable")
        .filter_map(std::result::Result::ok)
        .filter_map(|entry| {
            let file_name = entry.file_name();
            let file_name = file_name.to_str()?;

            if !file_name.ends_with(suffix) || !is_numbered_file_name(file_name) {
                return None;
            }

            Some(file_name.trim_end_matches(suffix).to_string())
        })
        .collect()
}

fn is_numbered_file_name(file_name: &str) -> bool {
    let mut characters = file_name.chars();

    matches!(characters.next(), Some(character) if character.is_ascii_digit())
        && matches!(characters.next(), Some(character) if character.is_ascii_digit())
        && matches!(characters.next(), Some(character) if character.is_ascii_digit())
        && characters.next() == Some('_')
}
