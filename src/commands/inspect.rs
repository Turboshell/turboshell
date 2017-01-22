use sodiumoxide::crypto::sign;
use std::fs::File;
use std::io;
use std::io::{Read, Write};
use rustc_serialize::base64::FromBase64;

use unpack;

pub fn main<W: Write>(out: &mut W, output_string: String, pubkey_string: String, archive_string: String) -> i32 {
    let mut stderr = io::stderr();

    let mut input: Box<Read> = if archive_string != "" {
        Box::new(File::open(archive_string).unwrap())
    } else {
        Box::new(io::stdin())
    };

    // Create PublicKey
    let pubkey_bytes = match pubkey_string.as_bytes().from_base64() {
        Ok(bytes) => bytes,
        Err(_) => {
            writeln!(&mut stderr, "error decoding pubkey \"{}\" as base64", pubkey_string).unwrap();
            return 1;
        }
    };
    let pubkey = match sign::PublicKey::from_slice(&pubkey_bytes) {
        Some(key) => key,
        None => {
            writeln!(&mut stderr, "error creating PublicKey").unwrap();
            return 1;
        }
    };

    // Verify and Unpack
    let tarball_bytes = match unpack::unpack(&mut input, pubkey) {
        Ok(value) => value,
        Err(e) => {
            writeln!(&mut stderr, "{}", e).unwrap();
            return 1;
        }
    };

    // Output the tarball
    let mut output: Box<Write> = if output_string != "" {
        Box::new(File::create(output_string).unwrap())
    } else {
        Box::new(out)
    };
    output.write_all(&tarball_bytes).unwrap();

    return 0;
}
