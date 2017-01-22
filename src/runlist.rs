use std::collections::BTreeMap;
use std::ffi::CString;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::result;

use libc;

use rustc_serialize::{Encoder, Encodable};
use toml;

use error::{Error, Result};

use resolve::{Executable, PackageRepository};

// Env has to be a BTreeMap instead of regular HashMap because it has
// to impliment the Hash trait so that Package can derive Hash.
// And Package needs Hash so that it can be put in HashMaps.
pub type Env = BTreeMap<String, String>;

#[derive(Hash, PartialEq, Eq, Clone, Debug)]
pub struct Package {
    pub dir: PathBuf,
    pub name: String,
    version: String,
    pub main: PathBuf,
    pub env: Env,
    dependencies: BTreeMap<String, String>
}

#[derive(Debug)]
pub struct Role {
    pub path: PathBuf,
    pub name: String,
    pub dependencies: Vec<Package>,
    pub env: Env
}

#[derive(Debug)]
pub struct RunList {
    pub basedir: PathBuf,
    repo: PackageRepository,
    pub roles: Vec<Role>,
}

fn read_toml<P: AsRef<Path>>(path: &P) -> Result<toml::Value> {
    let mut f = match File::open(&path) {
        Ok(v) => v,
        Err(_) => {return Err(Error::new(PathBuf::from(path.as_ref()), "failure to open"))}
    };
    let mut s = String::new();

    match f.read_to_string(&mut s) {
        Ok(_) => match s.parse() {
            Ok(v) => Ok(v),
            Err(_) => Err(Error::new(PathBuf::from(path.as_ref()), "failure to parse toml"))
        },
        Err(_) => Err(Error::new(PathBuf::from(path.as_ref()), "failure to read - perhaps invalid UTF-8?"))
    }
}

fn is_executable(path: &PathBuf) -> bool {
    let s: String = path.to_str().unwrap().into();
    let result = unsafe { libc::access(CString::new(s).unwrap().as_ptr(), libc::F_OK | libc::X_OK) };
    result == 0
}


impl Package {
    pub fn from_file<P: AsRef<Path>>(basedir: &P, name: &str) -> Result<Package> {
        let dir = basedir.as_ref().join(name);
        let config_path = dir.join("package.toml");
        let config = try!(read_toml(&config_path));

        let pname = match config.lookup("package.name") {
            Some(pname) => match pname.as_str() {
                Some(str) => str,
                None => return Err(Error::new(config_path, "package `name` isn't a string."))
            },
            None => return Err(Error::new(config_path, "package `name` is missing."))
        };

        if pname != name {
            return Err(Error::new(config_path, "package `name` doesn't match its directory."));
        }

        let main = dir.join(match config.lookup("package.main") {
            Some(main) => match main.as_str() {
                Some(str) => str,
                None => return Err(Error::new(config_path, "package `main` isn't a string."))
            },
            None => "main.sh"
        });

        if ! main.exists() {
            return Err(Error::new(config_path, "package `main` doesn't exist."));
        }

        if ! is_executable(&main) {
            return Err(Error::new(config_path, "package `main` isn't executable."));
        }

        let version = match config.lookup("package.version") {
            Some(version) => match version.as_str() {
                Some(str) => str,
                None => return Err(Error::new(config_path, "package `version` isn't a string."))
            },
            None => return Err(Error::new(config_path, "package `version` is missing."))
        };

        let env = match config.lookup("env") {
            Some(env) => match env.as_table() {
                Some(table) => {
                    let mut m = Env::new();

                    for (k, v) in table {
                        match v.as_str() {
                            Some(v) => {
                                m.insert(k.clone(),v.to_string());
                            },
                            None => return Err(Error::new(config_path, &format!("package `env` value \"{}\" isn't a string.", k)))
                        }
                    }

                    m
                },
                None => return Err(Error::new(config_path, "package `env` isn't a table."))
            },
            None => Env::new()
        };

        let dependencies = match config.lookup("package.dependencies") {
            Some(deps) => match deps.as_slice() {
                Some(slice) => {
                    let mut m = BTreeMap::new();

                    for name in slice {
                        match name.as_str() {
                            Some(v) => {
                                m.insert(v.to_string(), "local".to_string());
                            },
                            None => return Err(Error::new(config_path, &format!("package dependency \"{}\" isn't a string.", name)))
                        };

                    }
                    m
                },
                None => return Err(Error::new(config_path, "package `dependencies` isn't an array."))
            },
            None => BTreeMap::new()
        };

        Ok(Package{main: dir.join(main),
                   dir: dir,
                   name: name.to_string(),
                   version: version.to_string(),
                   env: env,
                   dependencies: dependencies})
    }

    pub fn dependencies(&self) -> &BTreeMap<String, String> {
        &self.dependencies
    }
}

impl fmt::Display for Package {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Package({})", self.name)
    }
}

impl Role {
    fn from_file<P: AsRef<Path>>(basedir: &P, role: &str) -> Result<Role> {
        let role_path = basedir.as_ref().join("roles").join(format!("{}.toml", role));
        let config = try!(read_toml(&role_path));

        let dependencies = match config.lookup("role.dependencies") {
            Some(deps) => match deps.as_slice() {
                Some(slice) => {
                    let mut v: Vec<Package> = Vec::with_capacity(slice.len());

                    for name in slice {
                        match name.as_str() {
                            Some(val) => {
                                v.push(try!(Package::from_file(basedir, val)));
                            },
                            None => return Err(Error::new(role_path, &format!("role dependency \"{}\" isn't a string.", name)))
                        };

                    }
                    v
                },
                None => return Err(Error::new(role_path, "role `dependencies` isn't an array."))
            },
            None => vec![]
        };

        let env = match config.lookup("env") {
            Some(env) => match env.as_table() {
                Some(table) => {
                    let mut m = Env::new();

                    for (k, v) in table {
                        match v.as_str() {
                            Some(v) => {
                                m.insert(k.clone(),v.to_string());
                            },
                            None => return Err(Error::new(role_path, "`env` values aren't strings."))
                        }
                    }

                    m
                },
                None => return Err(Error::new(role_path, "`env` key isn't a table."))
            },
            None => Env::new()
        };

        Ok(Role{path: role_path, name: role.to_string(), dependencies: dependencies, env: env})
    }

    pub fn dependencies(&self) -> &Vec<Package> {
        &self.dependencies
    }

    pub fn env(&self) -> &Env {
        &self.env
    }

}

impl Encodable for Role {
    fn encode<E: Encoder>(&self, e: &mut E) -> result::Result<(), E::Error> {
        e.emit_map(self.dependencies.len(), |e| {
            let mut i = 0;
            for package in self.dependencies.iter() {
                try!(e.emit_map_elt_key(i, |e| e.emit_str(&package.name)));
                try!(e.emit_map_elt_val(i, |e| e.emit_str(&package.version)));

                i += 1;
            }
            Ok(())
        })
    }
}

impl RunList {
    pub fn from_roles<P: AsRef<Path>>(basedir: &P, roles: &Vec<String>) -> Result<RunList> {
        let mut v = Vec::with_capacity(roles.len());
        for role in roles {
            v.push(try!(Role::from_file(basedir, role)));
        }
        Ok(RunList{ basedir: basedir.as_ref().to_path_buf(),
                    repo: try!(PackageRepository::from_basedir(basedir)),
                    roles: v })
    }

    pub fn from_archive<P: AsRef<Path>>(basedir: &P) -> Result<RunList> {
        let path = basedir.as_ref().join("archive.toml");
        let archive = try!(read_toml(&path));
        let roles = match archive.lookup("archive.roles") {
            Some(roles) => {
                match roles.as_slice() {
                    Some(slice) => {
                        let mut v = Vec::with_capacity(slice.len());
                        for name in slice {
                            match name.as_str() {
                                Some(str) => v.push(str.to_string()),
                                None => return Err(Error::new(path, "`roles` isn't an array of strings."))
                            };
                        }
                        v
                    },
                    None => return Err(Error::new(path, "`roles` key isn't an array."))
                }
            },
            None => vec![]
        };

        RunList::from_roles(basedir, &roles)
    }

    pub fn sort_dependencies(&self) -> Result<Vec<Executable>> {
        self.repo.resolve(&self.roles)
    }
}

impl Encodable for RunList {
    fn encode<E: Encoder>(&self, e: &mut E) -> result::Result<(), E::Error> {
        try!(e.emit_map(1, |e| {
            try!(e.emit_map_elt_key(0, |e| e.emit_str("archive")));
            e.emit_map_elt_val(0, |e| {
                e.emit_map(1, |e| {
                    try!(e.emit_map_elt_key(0, |e| e.emit_str("roles")));
                    try!(e.emit_map_elt_val(0, |e| {
                        e.emit_seq(self.roles.len(), |e| {
                            let mut i = 0;
                            for role in self.roles.iter() {
                                try!(e.emit_seq_elt(i, |e| e.emit_str(&role.name)));

                                i += 1;
                            }
                            Ok(())
                        })
                    }));
                    Ok(())
                })
            })
        }));

        try!(e.emit_map(1, |e| {
            try!(e.emit_map_elt_key(0, |e| e.emit_str("role")));
            e.emit_map_elt_val(0, |e| {
                e.emit_map(self.roles.len(), |e| {
                    let mut i = 0;
                    for role in self.roles.iter() {
                        try!(e.emit_map_elt_key(i, |e| e.emit_str(&role.name)));
                        try!(e.emit_map_elt_val(0, |e| role.encode(e)));

                        i += 1;
                    }
                    Ok(())
                })
            })
        }));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Env, Package};
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    fn resource<P: AsRef<Path>>(path: P) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("resources").join(path)
    }

    fn quick_package<P: AsRef<Path>>(basedir: P, name: &str, version: &str, main: &str) -> Package {
        Package{dir: basedir.as_ref().join(name),
                name: name.to_string(),
                version: version.to_string(),
                main: basedir.as_ref().join(name).join(main),
                env: Env::new(),
                dependencies: BTreeMap::new()}
    }

    #[test]
    fn package_with_no_problems() {
        let basedir = resource("package_unit_tests");
        let name = "no_problems";
        let p = Package::from_file(&basedir, name).unwrap();

        assert_eq!(p, quick_package(basedir, name, "17", "main.sh"));
    }

    #[test]
    fn package_with_main_specified() {
        let basedir = resource("package_unit_tests");
        let name = "main_specified";
        let p = Package::from_file(&basedir, name).unwrap();

        assert_eq!(p, quick_package(basedir, name, "25", "this_is_the_main.sh"));
    }

    #[test]
    fn package_dependencies() {
        let basedir = resource("package_unit_tests").join("dependencies");
        let pkg_a = Package::from_file(&basedir, "pkg_a").unwrap();
        let pkg_b = Package::from_file(&basedir, "pkg_b").unwrap();
        let pkg_c = Package::from_file(&basedir, "pkg_c").unwrap();
        let pkg_d = Package::from_file(&basedir, "pkg_d").unwrap();

        // a depends on b and d
        let mut a_deps = BTreeMap::new();
        a_deps.insert("pkg_b".to_string(), "local".to_string());
        a_deps.insert("pkg_d".to_string(), "local".to_string());
        assert_eq!(pkg_a.dependencies(), &a_deps);

        // b depends on c
        let mut b_deps = BTreeMap::new();
        b_deps.insert("pkg_c".to_string(), "local".to_string());
        assert_eq!(pkg_b.dependencies(), &b_deps);

        // c depends on d
        let mut c_deps = BTreeMap::new();
        c_deps.insert("pkg_d".to_string(), "local".to_string());
        assert_eq!(pkg_c.dependencies(), &c_deps);

        // d has no deps
        assert_eq!(0, pkg_d.dependencies.len());
    }

    #[test]
    fn package_env_vars() {
        let basedir = resource("package_unit_tests").join("env");

        let no_env = Package::from_file(&basedir, "no_env").unwrap();
        assert_eq!(no_env.env.len(), 0);

        let basic_env = Package::from_file(&basedir, "basic_env").unwrap();
        let mut basic_env_env = Env::new();
        basic_env_env.insert("OK".to_string(), "ok".to_string());
        basic_env_env.insert("FOO".to_string(), "green eggs and ham".to_string());
        assert_eq!(basic_env.env, basic_env_env);
    }

    // ERROR CASES

    #[test]
    fn package_directory_is_missing() {
        let basedir = resource("package_unit_tests");
        let name = "THIS_DIRECTORY_IS_MISSING";
        let p = Package::from_file(&basedir, name);

        assert!(p.is_err());

        let err = p.err().unwrap();
        assert_eq!(err.path(), basedir.join(name).join("package.toml"));
        assert_eq!(err.message(), "failure to open");
    }

    #[test]
    fn package_with_unreadable_package_toml() {
        let basedir = resource("package_unit_tests");
        let name = "unreadable_package_toml";

        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        // remove read (well... all) permissions from package.toml
        // I'm doing this in code here because git doesn't store permissions aside from +x
        let package_toml_file = basedir.join(name).join("package.toml");
        let original_mode = fs::metadata(&package_toml_file).unwrap().permissions().mode();

        fs::set_permissions(&package_toml_file, fs::Permissions::from_mode(0o000)).unwrap();

        let p = Package::from_file(&basedir, name);

        // return original mode
        fs::set_permissions(&package_toml_file, fs::Permissions::from_mode(original_mode)).unwrap();

        assert!(p.is_err());

        let err = p.err().unwrap();
        assert_eq!(err.path(), basedir.join(name).join("package.toml"));
        assert_eq!(err.message(), "failure to open");
    }

    #[test]
    fn package_has_broken_toml() {
        let basedir = resource("package_unit_tests");
        let name = "broken_toml";
        let p = Package::from_file(&basedir, name);

        assert!(p.is_err());

        let err = p.err().unwrap();
        assert_eq!(err.path(), basedir.join(name).join("package.toml"));
        assert_eq!(err.message(), "failure to parse toml");
    }

    #[test]
    fn package_with_invalid_utf8() {
        let basedir = resource("package_unit_tests");
        let name = "invalid_utf8";
        let p = Package::from_file(&basedir, name);

        assert!(p.is_err());

        let err = p.err().unwrap();
        assert_eq!(err.path(), basedir.join(name).join("package.toml"));
        assert_eq!(err.message(), "failure to read - perhaps invalid UTF-8?");
    }

    #[test]
    fn package_name_isnt_a_string() {
        let basedir = resource("package_unit_tests");
        let name = "name_isnt_a_string";
        let p = Package::from_file(&basedir, name);

        assert!(p.is_err());

        let err = p.err().unwrap();
        assert_eq!(err.path(), basedir.join(name).join("package.toml"));
        assert_eq!(err.message(), "package `name` isn't a string.");
    }

    #[test]
    fn package_name_is_missing() {
        let basedir = resource("package_unit_tests");
        let name = "name_is_missing";
        let p = Package::from_file(&basedir, name);

        assert!(p.is_err());

        let err = p.err().unwrap();
        assert_eq!(err.path(), basedir.join(name).join("package.toml"));
        assert_eq!(err.message(), "package `name` is missing.");
    }

    #[test]
    fn package_name_doesnt_match_directory() {
        let basedir = resource("package_unit_tests");
        let name = "name_doesnt_match_directory";
        let p = Package::from_file(&basedir, name);

        assert!(p.is_err());

        let err = p.err().unwrap();
        assert_eq!(err.path(), basedir.join(name).join("package.toml"));
        assert_eq!(err.message(), "package `name` doesn't match its directory.");
    }

    #[test]
    fn package_main_isnt_a_string() {
        let basedir = resource("package_unit_tests");
        let name = "main_isnt_a_string";
        let p = Package::from_file(&basedir, name);

        assert!(p.is_err());

        let err = p.err().unwrap();
        assert_eq!(err.path(), basedir.join(name).join("package.toml"));
        assert_eq!(err.message(), "package `main` isn't a string.");
    }

    #[test]
    fn package_main_doesnt_exist() {
        let basedir = resource("package_unit_tests");
        let name = "main_doesnt_exist";
        let p = Package::from_file(&basedir, name);

        assert!(p.is_err());

        let err = p.err().unwrap();
        assert_eq!(err.path(), basedir.join(name).join("package.toml"));
        assert_eq!(err.message(), "package `main` doesn't exist.");
    }

    #[test]
    fn package_main_isnt_executable() {
        let basedir = resource("package_unit_tests");
        let name = "main_isnt_executable";
        let p = Package::from_file(&basedir, name);

        assert!(p.is_err());

        let err = p.err().unwrap();
        assert_eq!(err.path(), basedir.join(name).join("package.toml"));
        assert_eq!(err.message(), "package `main` isn't executable.");
    }

    #[test]
    fn package_version_isnt_a_string() {
        let basedir = resource("package_unit_tests");
        let name = "version_isnt_a_string";
        let p = Package::from_file(&basedir, name);

        assert!(p.is_err());

        let err = p.err().unwrap();
        assert_eq!(err.path(), basedir.join(name).join("package.toml"));
        assert_eq!(err.message(), "package `version` isn't a string.");
    }

    #[test]
    fn package_version_is_missing() {
        let basedir = resource("package_unit_tests");
        let name = "version_is_missing";
        let p = Package::from_file(&basedir, name);

        assert!(p.is_err());

        let err = p.err().unwrap();
        assert_eq!(err.path(), basedir.join(name).join("package.toml"));
        assert_eq!(err.message(), "package `version` is missing.");
    }

    #[test]
    fn package_env_values_arent_strings() {
        let basedir = resource("package_unit_tests");
        let name = "env_values_arent_strings";
        let p = Package::from_file(&basedir, name);

        assert!(p.is_err());

        let err = p.err().unwrap();
        assert_eq!(err.path(), basedir.join(name).join("package.toml"));
        assert_eq!(err.message(), "package `env` value \"baz\" isn't a string.");
    }

    #[test]
    fn package_env_isnt_a_table() {
        let basedir = resource("package_unit_tests");
        let name = "env_isnt_a_table";
        let p = Package::from_file(&basedir, name);

        assert!(p.is_err());

        let err = p.err().unwrap();
        assert_eq!(err.path(), basedir.join(name).join("package.toml"));
        assert_eq!(err.message(), "package `env` isn't a table.");
    }

    #[test]
    fn package_dependencies_isnt_an_array() {
        let basedir = resource("package_unit_tests");
        let name = "dependencies_isnt_an_array";
        let p = Package::from_file(&basedir, name);

        assert!(p.is_err());

        let err = p.err().unwrap();
        assert_eq!(err.path(), basedir.join(name).join("package.toml"));
        assert_eq!(err.message(), "package `dependencies` isn't an array.");
    }

    #[test]
    fn package_dependencies_isnt_an_array_of_strings() {
        let basedir = resource("package_unit_tests");
        let name = "dependencies_isnt_an_array_of_strings";
        let p = Package::from_file(&basedir, name);

        assert!(p.is_err());

        let err = p.err().unwrap();
        assert_eq!(err.path(), basedir.join(name).join("package.toml"));
        assert_eq!(err.message(), "failure to parse toml");
    }
}
