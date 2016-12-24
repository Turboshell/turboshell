use flate2::Compression;
use flate2::write::GzEncoder;
use sodiumoxide::crypto::sign;
use std::path::{Path, PathBuf};

use rustc_serialize::base64;
use rustc_serialize::base64::ToBase64;

use std::fs::File;
use std::io;
use std::io::Write;

use tar;
use toml;
use cast;

use seedfile;
use runlist;

use walkdir::{DirEntry, WalkDir, WalkDirIterator};

use std::os::unix::fs::MetadataExt;

fn is_hidden(entry: &DirEntry) -> bool {
    return entry.file_name()
        .to_str()
        .map(|s| s.starts_with("."))
        .unwrap_or(false);
}

fn sign(bytes: &[u8], sk: &sign::SecretKey) -> String {
    let signature = sign::sign_detached(&bytes, &sk);
    let signature_bytes: &[u8] = signature.as_ref();
    return signature_bytes.to_base64(base64::STANDARD);
}

fn write_to_archive<P: AsRef<Path>, W: Write>(builder: &mut tar::Builder<W>, basedir_with_slash: &String, entry_path: P) -> io::Result<()> {
    let entry_str = match entry_path.as_ref().to_str() {
        Some(v) => v,
        None => {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "can't make a string"));
        }
    };
    let entry_name = entry_str.replace(basedir_with_slash, "");

    if entry_path.as_ref().is_dir() {
        // write this directory to the archive
        let mut header = tar::Header::new_gnu();
        try!(header.set_path(entry_name + "/"));
        header.set_size(0);
        header.set_mode(try!(entry_path.as_ref().metadata()).mode());
        header.set_entry_type(tar::EntryType::dir());
        header.set_cksum();
        try!(builder.append(&header, io::empty()));
    } else {
        // write this file to the archive
        let mut header = tar::Header::new_gnu();
        let metadata = try!(entry_path.as_ref().metadata());
        try!(header.set_path(entry_name));
        header.set_size(metadata.len());
        header.set_mode(metadata.mode());
        header.set_cksum();
        try!(builder.append(&header, try!(File::open(entry_path.as_ref()))));
    }

    Ok(())
}

impl runlist::RunList {
    pub fn write<W: Write>(&self, mut out: &mut W) -> io::Result<()> {
        let basedir_with_slash = match self.basedir.to_str() {
            Some(v) => v.to_string() + "/",
            None => {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "can't make a string"));
            }
        };
        let mut builder = tar::Builder::new(&mut out);

        // create the archive.toml
        let archive_toml_contents = toml::encode_str(self);
        let mut header = tar::Header::new_gnu();
        try!(header.set_path("archive.toml"));
        header.set_size(cast::u64(archive_toml_contents.len()));
        header.set_mode(0o444u32);
        header.set_cksum();
        try!(builder.append(&header, io::Cursor::new(archive_toml_contents.as_bytes())));

        // create a roles/ directory
        try!(write_to_archive(&mut builder, &basedir_with_slash, PathBuf::from(&self.basedir).join("roles")));


        // write the role file for all roles
        for role in self.roles.iter() {
            try!(write_to_archive(&mut builder, &basedir_with_slash, &role.path));
        }

        // now write all (non hidden) files for all deps of this runlist
        match self.sort_dependencies() {
            Ok(deps) => {
                for dep in deps {
                    let walker = WalkDir::new(dep.dir).into_iter();
                    for entry in walker.filter_entry(|e| !is_hidden(e)) {
                        let entry = try!(entry);
                        let entry_path = entry.path();

                        try!(write_to_archive(&mut builder, &basedir_with_slash, entry_path));
                    }
                }
            },
            Err(e) => {
                return Err(io::Error::new(io::ErrorKind::InvalidData, e.message()));
            }
        }

        try!(builder.into_inner());

        Ok(())
    }
}

pub fn main<W: Write>(out: &mut W, basedir_string: String, output_string: String, seedfile_string: String, roles: Vec<String>) -> i32 {
    let mut stderr = io::stderr();

    let basedir = match PathBuf::from(&basedir_string).canonicalize() {
        Ok(value) => value,
        Err(e) => {
            writeln!(&mut stderr, "Can't find directory {}: {}", basedir_string, e).unwrap();
            return 1;
        },
    };

    let seedfile_path = match PathBuf::from(&seedfile_string).canonicalize() {
        Ok(value) => value,
        Err(e) => {
            writeln!(&mut stderr, "Can't find seedfile {}: {}", seedfile_string, e).unwrap();
            return 1;
        },
    };

    let (_, sk) = {
        let seedfile = match seedfile::SeedFile::from_path(seedfile_path) {
            Ok(v) => v,
            Err(e) => {
                writeln!(&mut stderr, "{}", e).unwrap();
                return 1;
            }
        };
        seedfile.keypair()
    };

    let runlist = match runlist::RunList::from_roles(&basedir, &roles) {
        Ok(v) => v,
        Err(e) => {
            writeln!(&mut stderr, "{}", e).unwrap();
            return 1;
        }
    };

    //////// CREATE THE TARBALL /////////
    let mut tarball_bytes = Vec::new();
    { // scope to release mut borrow of tarball_bytes
        let mut gzipper = GzEncoder::new(&mut tarball_bytes, Compression::Default);
        { // new scope for mut borrow of gzipper
            runlist.write(&mut gzipper).unwrap();

        }
        gzipper.finish().unwrap();
    }


    let signature = sign(&tarball_bytes, &sk);



    //////// WRITE OUT THE ARCHIVE //////////
    let mut output: Box<Write> = if output_string != "" {
        Box::new(File::create(output_string).unwrap())
    } else {
        Box::new(out)
    };
    // let mut output = File::create(args.flag_output).unwrap();
       
    output.write_all(b"TURBOv01").unwrap();
    output.write_all(signature.as_bytes()).unwrap();
    output.write_all(&tarball_bytes).unwrap();

    return 0;
}
