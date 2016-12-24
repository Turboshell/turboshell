use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use crc::crc32;
use rustc_serialize::base64;
use rustc_serialize::base64::{FromBase64, ToBase64};
use sodiumoxide::crypto::sign;
use sodiumoxide::randombytes;
use std::fs::File;
use std::io;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use error::{Error, Result};

pub struct SeedFile {
    seed: sign::Seed
}

impl SeedFile {
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<SeedFile> {
        SeedFile::from_reader(&mut BufReader::new(match File::open(&path) {
            Ok(v) => v,
            Err(_) => return Err(Error::new(PathBuf::from(path.as_ref()), "Invalid Seedfile. Couldn't open."))
        }), path.as_ref().clone())
    }

    pub fn from_reader<BR: BufRead, P: AsRef<Path>>(input: &mut BR, source: P) -> Result<SeedFile> {
        let source = PathBuf::from(source.as_ref());

        let lines: Vec<String> = match input.lines().collect::<io::Result<Vec<String>>>() {
            Ok(value) => value,
            Err(_) => {
                return Err(Error::new(source, "Invalid Seedfile. Couldn't read."));
            }
        };

        // there must be exactly three lines
        if lines.len() != 3 {
            return Err(Error::new(source, "Invalid Seedfile. Not 3 lines."));
        }

        // the top line must be right
        let top_line = match lines.get(0) {
            Some(value) => value,
            None => {
                return Err(Error::new(source, "Invalid Seedfile. Missing top line? Somehow?"));
            }
        };
        if top_line != "---------- THIS IS YOUR PRIVATE SEED FILE ----------" {
            return Err(Error::new(source, "Invalid Seedfile. Invalid line 1."));
        }

        // the bottom line must be right
        let bottom_line = match lines.get(2) {
            Some(value) => value,
            None => {
                return Err(Error::new(source, "Invalid Seedfile. Missing bottom line? Somehow?"));
            }
        };
        if bottom_line != "------------- DO NOT SHARE IT PUBLICLY -------------" {
            return Err(Error::new(source, "Invalid Seedfile. Invalid line 3."));
        }

        // now we check the key itself
        let middle_line = match lines.get(1) {
            Some(value) => value,
            None => {
                return Err(Error::new(source, "Invalid Seedfile. Missing middle line? Somehow?"));
            }
        };

        // check line length
        if middle_line.len() != 52 {
            return Err(Error::new(source, "Invalid Seedfile. Line 2 too short."));
        }

        let (seed_base64, crc_base64) = middle_line.as_bytes().split_at(44);


        // decode the seed from base64
        let seedbuf = match seed_base64.from_base64() {
            Ok(value) => value,
            Err(_) => {
                return Err(Error::new(source, "Invalid Seedfile. Seed failed base64 decode."));
            },
        };

        //decode the crc from base64
        let crcbuf = match crc_base64.from_base64() {
            Ok(value) => value,
            Err(_) => {
                return Err(Error::new(source, "Invalid Seedfile. CRC failed base64 decode."));
            },
        };

        //check the stored crc against a freshly computed one
        let stored_crc: u32 = match io::Cursor::new(crcbuf).read_u32::<BigEndian>() {
            Ok(v) => v,
            Err(_) => return Err(Error::new(source, "Invalid Seedfile. CRC failed Big Endian decode."))
        };
        let computed_crc: u32 = crc32::checksum_ieee(&seedbuf);

        if stored_crc != computed_crc {
            return Err(Error::new(source, "Invalid Seedfile. CRC does not match."));
        }

        // all checks pass! now just construct a crypto::Seed and derive keys
        let mut seedarray = [0u8; sign::SEEDBYTES];
        for (i, byte) in  seedbuf.into_iter().enumerate() {
            seedarray[i] = byte;
        }

        Ok(SeedFile{seed: sign::Seed(seedarray)})
    }

    pub fn new() -> SeedFile {
        let mut seedbuf = [0; sign::SEEDBYTES];
        randombytes::randombytes_into(&mut seedbuf);

        SeedFile{seed: sign::Seed(seedbuf)}
    }

    pub fn to_string(&self) -> io::Result<String> {
        let sign::Seed(seedbuf) = self.seed;
        let crc: u32 = crc32::checksum_ieee(&seedbuf);
        let mut v = Vec::new();
        try!(v.write_u32::<BigEndian>(crc));

        Ok(format!("{}\n{}{}\n{}",
                   "---------- THIS IS YOUR PRIVATE SEED FILE ----------",
                   seedbuf.to_base64(base64::STANDARD), v.to_base64(base64::STANDARD),
                   "------------- DO NOT SHARE IT PUBLICLY -------------"))
    }

    pub fn keypair(&self) -> (sign::PublicKey, sign::SecretKey) {
        sign::keypair_from_seed(&self.seed)
    }

}

#[cfg(test)]
mod tests {
    use super::SeedFile;
    use error::Result;
    use std::io::BufReader;
    use std::path::Path;
    use sodiumoxide;

    fn seedfile_from_str<S: Into<String>>(s: S) -> Result<SeedFile> {
        let str = s.into();
        let mut br = BufReader::new(str.as_bytes());
        SeedFile::from_reader(&mut br, Path::new("<stdin>"))
    }

    #[test]
    fn new_seedfile_each_time() {
        if ! sodiumoxide::init() {
            panic!("Failed to init libsodium");
        }

        let a = SeedFile::new();
        let b = SeedFile::new();
        assert!(a.seed != b.seed);
        assert!(a.to_string().unwrap() != b.to_string().unwrap());
    }

    #[test]
    fn roundtrip_seedfile() {
        if ! sodiumoxide::init() {
            panic!("Failed to init libsodium");
        }

        let a = SeedFile::new();
        let b = seedfile_from_str(a.to_string().unwrap()).unwrap();
        assert_eq!(a.seed, b.seed);
        assert_eq!(a.to_string().unwrap(), b.to_string().unwrap());
    }

    #[test]
    fn test_parsing() {
        let correct = SeedFile::new().to_string().unwrap();

        assert_eq!(seedfile_from_str("totally invalid seedfile").err().unwrap().message(), "Invalid Seedfile. Not 3 lines.");

        let ex = correct.clone().replace("THIS IS YOUR PRIVATE SEED FILE", "foobar");
        assert_eq!(seedfile_from_str(ex).err().unwrap().message(), "Invalid Seedfile. Invalid line 1.");

        let ex = correct.clone().replace("DO NOT SHARE IT PUBLICLY", "foobar");
        assert_eq!(seedfile_from_str(ex).err().unwrap().message(), "Invalid Seedfile. Invalid line 3.");

        let ex = correct.clone().replace("DO NOT SHARE IT PUBLICLY", "foobar");
        assert_eq!(seedfile_from_str(ex).err().unwrap().message(), "Invalid Seedfile. Invalid line 3.");

        let lines = correct.lines().collect::<Vec<&str>>();

        assert_eq!(seedfile_from_str([lines[0], "foobar", lines[2]].join("\n")).err().unwrap().message(), "Invalid Seedfile. Line 2 too short.");
        assert_eq!(seedfile_from_str([lines[0], "0123^&*(89012345678901234567890123456789012345678901", lines[2]].join("\n")).err().unwrap().message(), "Invalid Seedfile. Seed failed base64 decode.");
        assert_eq!(seedfile_from_str([lines[0], "0123456789012345678901234567890123456789012345^&*(01", lines[2]].join("\n")).err().unwrap().message(), "Invalid Seedfile. CRC failed base64 decode.");
        assert_eq!(seedfile_from_str([lines[0], "0123456789012345678901234567890123456789012345678901", lines[2]].join("\n")).err().unwrap().message(), "Invalid Seedfile. CRC does not match.");
    }
}
