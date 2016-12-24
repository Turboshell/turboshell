use libc;
use rustc_serialize::base64;
use rustc_serialize::base64::ToBase64;

use std::fs::File;
use std::io;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use seedfile;

fn has_stdin() -> bool {
    unsafe { return libc::isatty(libc::STDIN_FILENO) == 0 };
}

pub fn main<W: Write>(out: &mut W, seedfile_string: String, output_string: String) -> i32{
    let mut stderr = io::stderr();
    let stdin = io::stdin();
    let should_read_from_file: bool = seedfile_string != "";

    if should_read_from_file || has_stdin() {
        let (mut input, source): (Box<BufRead>, PathBuf) = if should_read_from_file {
            (Box::new(BufReader::new(match File::open(&seedfile_string) {
                Ok(value) => value,
                Err(e) => {
                    writeln!(&mut stderr, "{}: {}", e, seedfile_string).unwrap();
                    return 1;
                }
            })),
             PathBuf::from(seedfile_string))
        } else {
            (Box::new(stdin.lock()), PathBuf::from("<stdin>"))
        };

        match seedfile::SeedFile::from_reader(&mut input, source) {
            Ok(seedfile) => {
                let (pk, _) = seedfile.keypair();
                let pk_bytes: &[u8] = pk.as_ref();
                writeln!(out, "{}", pk_bytes.to_base64(base64::STANDARD)).unwrap();
                return 0;
            }
            Err(e) => {
                writeln!(&mut stderr, "{}", e).unwrap();
                return 1;
            }
        }
    } else {
        let mut output: Box<Write> = if output_string != "" {
            Box::new(File::create(output_string).unwrap())
        } else {
            Box::new(io::stdout())
        };

        match seedfile::SeedFile::new().to_string() {
            Ok(v) => {
                writeln!(&mut output, "{}", v).unwrap();
                return 0;
            },
            Err(e) => {
                writeln!(&mut stderr, "{}", e).unwrap();
                return 1;
            }
        }
    }
}
