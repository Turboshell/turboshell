extern crate chrono;

use std::process::Command;
use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;

fn main() {
    let sha: String = {
        let mut command = Command::new("git");
        command.args(&["rev-parse", "--short", "HEAD"]);
        let output = command.output().unwrap();
        String::from_utf8(output.stdout).unwrap()
    };

    let dirty: &str = {
        let mut command = Command::new("git");
        command.args(&["diff-index", "--quiet", "HEAD", "--"]);
        let status = command.status().unwrap();
        if status.success() {
            ""
        } else {
            "*"
        }
    };

    let mut f = {
        let out_dir = env::var("OUT_DIR").unwrap();
        let dest_path = Path::new(&out_dir).join("BUILD.rs");
        File::create(&dest_path).unwrap()
    };

    f.write_all(format!("const BUILD_GIT_SHA: &'static str = \"{}{}\";\n", sha.trim(), dirty).as_bytes()).unwrap();
    f.write_all(format!("const BUILD_DATETIME: &'static str = \"{}\";\n", chrono::UTC::now().to_rfc2822()).as_bytes()).unwrap();
}
