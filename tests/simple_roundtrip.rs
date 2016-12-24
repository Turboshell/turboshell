extern crate turboshell;
extern crate sodiumoxide;
extern crate tempdir;

use turboshell::commands::{keytool, compile, inspect, run};
use std::fs;
use std::io;
use std::io::Read;
use std::path::{Path, PathBuf};

fn resource<P: AsRef<Path>>(path: P) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("resources").join(path)
}

#[test]
fn main() {
    // create a seedfile
    // verify it
    // compile an archive
    // inspect the archive?
    // run the archive
    // check for expected side effects

    let tempdir = tempdir::TempDir::new("simple_roundtrip").unwrap();
    let test_output_file = PathBuf::from("/tmp").join("turboshell_integration_tests_simple_roundtrip");
    let _ = fs::remove_file(&test_output_file);
    assert!(!test_output_file.exists());

    if ! sodiumoxide::init() {
        panic!("Failed to init libsodium");
    }

    ///////////////////////
    // create a seedfile //
    ///////////////////////
    let seedfile = tempdir.path().join("seedfile");
    let mut output = io::Cursor::new(Vec::new());
    assert_eq!(keytool(&mut output,
                       "".to_string(),
                       seedfile.to_str().unwrap().to_string()),
               0);
    // check that it was written
    assert!(seedfile.exists());
    // read it back in with keytool
    let mut output = io::Cursor::new(Vec::new());
    assert_eq!(keytool(&mut output,
                       seedfile.to_str().unwrap().to_string(),
                       "".to_string()),
               0);
    let pubkey = String::from_utf8(output.into_inner()).unwrap();

    ////////////////////////
    // compile an archive //
    ////////////////////////
    let archive_path = tempdir.path().join("archive.tsar");
    let mut output = io::Cursor::new(Vec::new());
    assert_eq!(compile(&mut output,
                       resource("integration_tests").join("simple_roundtrip").to_str().unwrap().to_string(),
                       archive_path.to_str().unwrap().to_string(),
                       seedfile.to_str().unwrap().to_string(),
                       vec!["first".to_string(), "second".to_string()]),
               0);
    assert!(archive_path.exists());

    /////////////////////////
    // inspect the archive //
    /////////////////////////
    let mut output = io::Cursor::new(Vec::new());
    assert_eq!(inspect(&mut output,
                       "".to_string(),
                       pubkey.clone(),
                       archive_path.to_str().unwrap().to_string()),
               0);

    /////////////////////
    // run the archive //
    /////////////////////
    let mut output = io::Cursor::new(Vec::new());
    assert!(!test_output_file.exists());
    assert_eq!(run(&mut output,
                   pubkey.clone(),
                   archive_path.to_str().unwrap().to_string()),
               0);
    assert!(test_output_file.exists());

    /////////////////////////////////////
    // check for expected side effects //
    /////////////////////////////////////
    let mut f = fs::File::open(test_output_file).unwrap();
    let mut output = String::new();
    f.read_to_string(&mut output).unwrap();
    assert_eq!(output, r#"common foo = foo from first role
common bar =
a foo = foo from first role
a bar = bar from first role
b foo = foo from second role
b bar = bar from package
"#);

}
