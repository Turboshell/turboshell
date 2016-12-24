use flate2::read::GzDecoder;
use rustc_serialize::base64::FromBase64;
use sodiumoxide::crypto::sign;
use std::fs;
use std::io;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::os::unix::fs::PermissionsExt;
use tar;

pub fn unpack<R: Read>(input: &mut R, pubkey: sign::PublicKey) -> io::Result<Vec<u8>>{
    // Read & Verify Package Header
    let mut identifier_bytes = [0u8; 8];
    try!(input.read_exact(&mut identifier_bytes));

    if &identifier_bytes != b"TURBOv01" {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid Archive Header"));
    }

    // Read Signature
    let mut signature_bytes_base64 = [0u8; 88];
    try!(input.read_exact(&mut signature_bytes_base64));
    let signature_bytes = match (&signature_bytes_base64).from_base64() {
        Ok(value) => value,
        Err(e) => {
            return Err(io::Error::new(io::ErrorKind::InvalidData, e));
        }
    };
    let signature = match sign::Signature::from_slice(&signature_bytes) {
        Some(value) => value,
        None => {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid Signature"));
        }
    };

    // Read Tarball
    let mut tarball_bytes = Vec::new();
    try!(input.read_to_end(&mut tarball_bytes));

    // Verify Signature
    if ! sign::verify_detached(&signature, &tarball_bytes, &pubkey) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Signature Does Not Match"));
    }

    return Ok(tarball_bytes);
}

pub fn explode<R: Read, P: AsRef<Path>>(input: R, basedir: P) {
    let decoder = GzDecoder::new(input).unwrap();

    for iter_entry in tar::Archive::new(decoder).entries().unwrap() {
        let mut entry = iter_entry.unwrap();
        let new_path = PathBuf::from(basedir.as_ref()).join(&entry.path().unwrap());

        match entry.header().entry_type() {
            tar::EntryType::Directory => {
                fs::create_dir(&new_path).unwrap();
                fs::set_permissions(&new_path, fs::Permissions::from_mode(entry.header().mode().unwrap())).unwrap();
            },
            tar::EntryType::Regular => {
                let mut outfile = fs::File::create(&new_path).unwrap();
                io::copy(&mut entry, &mut outfile).unwrap();
                fs::set_permissions(&new_path, fs::Permissions::from_mode(entry.header().mode().unwrap())).unwrap();
            },
            default @ _ => panic!("unknown entry_type {:?}", default),
        }

    }
}
