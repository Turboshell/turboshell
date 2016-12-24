extern crate turboshell;
extern crate docopt;
extern crate rustc_serialize;
extern crate sodiumoxide;
extern crate libc;

use std::io;

const USAGE: &'static str = "
Turboshell

Usage:
  tsh keytool [ -o FILE | <seedfile> ]
  tsh compile [ -d DIR ] [ -o FILE ] -s FILE <role>...
  tsh inspect [ -o FILE ] -k KEY [ <archive> ]
  tsh run -k KEY [ <archive> ]
  tsh --help
  tsh --version

Options:
  -s FILE, --seedfile=FILE  PK seed file
  -k KEY, --key=KEY         public key
  -d DIR, --directory=DIR   directory [default: ./]
  -o FILE, --output=FILE    output file
  -h, --help                print this help message
  -V, --version             print the version of this program
";

#[derive(Debug,RustcDecodable)]
struct Args {
    arg_archive: String,
    arg_seedfile: String,
    arg_role: Vec<String>,
    cmd_keytool: bool,
    cmd_compile: bool,
    cmd_inspect: bool,
    cmd_run: bool,
    flag_seedfile: String,
    flag_key: String,
    flag_directory: String,
    flag_output: String,
    flag_version: bool,
}

include!(concat!(env!("OUT_DIR"), "/BUILD.rs"));

fn main() {
    let args: Args = docopt::Docopt::new(USAGE)
        .and_then(|d| d.decode())
        .unwrap_or_else(|e| e.exit());

    if args.flag_version {
        println!("turboshell {}-{} built on {}", env!("CARGO_PKG_VERSION"), BUILD_GIT_SHA, BUILD_DATETIME);
        println!("libsodium {}", sodiumoxide::version::version_string());
    } else {
        if ! sodiumoxide::init() {
            panic!("Failed to init libsodium");
        }

        let mut out = io::stdout();

        let exit_code = if args.cmd_keytool {
            turboshell::commands::keytool(&mut out, args.arg_seedfile, args.flag_output)
        } else if args.cmd_compile {
            turboshell::commands::compile(&mut out, args.flag_directory, args.flag_output, args.flag_seedfile, args.arg_role)
        } else if args.cmd_inspect {
            turboshell::commands::inspect(&mut out, args.flag_output, args.flag_key, args.arg_archive)
        } else if args.cmd_run {
            turboshell::commands::run(&mut out, args.flag_key, args.arg_archive)
        } else {
            unreachable!()
        };

        if exit_code != 0 {
            // Does it matter at all to ::exit with 0?
            std::process::exit(exit_code);
        }
    }
}
