use rustc_serialize::base64::FromBase64;
use std::fs::File;
use std::path::Path;
use std::process::{Command, Stdio};
use std::io;
use std::io::{BufRead, Read, Write};
use std::iter;
use sodiumoxide::crypto::sign;

use tempdir;

use unpack;
use resolve;
use runlist;

impl runlist::RunList {
    pub fn run<W: Write>(&self, out: &mut W) -> io::Result<()> {
        match self.sort_dependencies() {
            Ok(deps) => {
                writeln!(out, "Running: {}", deps.iter().map(|p| p.name.clone()).collect::<Vec<String>>().join(", ")).unwrap();

                for package in deps {
                    try!(package.run(out));
                }

                Ok(())
            },
            Err(e) => {
                Err(io::Error::new(io::ErrorKind::InvalidData, e.message()))
            }
        }
    }
}

fn run_command<P: AsRef<Path>, W: Write>(out: &mut W, main: &str, dir: P, env: &runlist::Env) -> io::Result<()>{
    let mut command = Command::new(main);
    command.current_dir(dir);
    //command.env_clear();
    for (k, v) in env {
        command.env(k,v);
    }
    command.stdout(Stdio::piped());
    command.stderr(Stdio::inherit()); // TODO: is this right?
    let mut child = match command.spawn() {
        Ok(v) => v,
        Err(e) => {
            return Err(io::Error::new(io::ErrorKind::Other, format!("error running {}: {}", main, e)));
        }
    };

    if let Some(ref mut stdout) = child.stdout {
        for line in io::BufReader::new(stdout).lines() {
            writeln!(out, "{}", try!(line)).unwrap();
        }
    }

    let status = try!(child.wait());

    if ! status.success() {
        Err(io::Error::new(io::ErrorKind::Other, format!("{} returned {}", main, status)))
    } else {
        Ok(())
    }
}

impl resolve::Executable {
    pub fn run<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let text = format!("##    Running Package {}    ##", self.name);
        let line = iter::repeat("#").take(text.len()).collect::<String>();
        writeln!(out, "\n{}\n{}\n{}\n", line, text, line).unwrap();
        run_command(out, self.main.to_str().unwrap(), &self.dir, &self.env)
    }
}

pub fn main<W: Write>(out: &mut W, pubkey_string: String, archive_string: String) -> i32 {
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

    // Create a place for the archive to be unpacked
    let tempdir = match tempdir::TempDir::new("turboshell") {
        Ok(value) => value,
        Err(e) => {
            writeln!(&mut stderr, "error creating temp dir: {}", e).unwrap();
            return 1;
        }
    };

    let basedir = match tempdir.path().canonicalize() {
        Ok(value) => value,
        Err(e) => {
            writeln!(&mut stderr, "Can't canonicalize temp dir: {}", e).unwrap();
            return 1;
        }
    };

    let tarball_bytes = unpack::unpack(&mut input, pubkey).unwrap();
    unpack::explode(tarball_bytes.as_slice(), &basedir);

    let runlist = match runlist::RunList::from_archive(&basedir) {
        Ok(v) => v,
        Err(e) => {
            writeln!(&mut stderr, "error reading archive: {}", e).unwrap();
            return 1;
        }
    };

    if let Err(e) = runlist.run(out) {
        writeln!(&mut stderr, "error running archive: {}", e).unwrap();
        return 1;
    }

    return 0;
}
