use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir, WalkDirIterator};

use error::{Error, Result};
use runlist::{Env, Package, Role};

struct Stack<T> {
    level: usize,
    vec: Vec<Vec<T>>,
    saved: Vec<VecDeque<T>>
}

impl<T> Stack<T> {
    fn new() -> Stack<T> {
        let deque = VecDeque::new();
        Stack{ level: 0,
               vec: vec![vec![]],
               saved: vec![deque] }
    }

    fn push_saved(&mut self, value: T) {
        self.saved[self.level].push_front(value)
    }

    fn pop_saved(&mut self) -> Option<T> {
        self.saved[self.level].pop_back()
    }

    #[cfg(test)]
    fn get_saved(&self) -> &[T] {
        self.saved[self.level].as_slices().0
    }

    fn push(&mut self, value: T) {
        self.vec[self.level].push(value)
    }

    fn pop(&mut self) -> Option<T> {
        self.vec[self.level].pop()
    }

    #[cfg(test)]
    fn get(&self) -> &[T] {
        self.vec[self.level].as_slice()
    }

    fn at_top(&self) -> bool {
        self.level == 0
    }

    fn indent(&mut self) {
        self.level += 1;
        self.vec.push(Vec::new());
        self.saved.push(VecDeque::new());
    }

    fn outdent(&mut self) -> Result<()> {
        self.vec.remove(self.level);
        self.saved.remove(self.level);
        if self.at_top() {
            Err(Error::new(PathBuf::from("undefined"), "already at top level"))
        } else {
            self.level -= 1;
            Ok(())
        }
    }
}

#[derive(Debug)]
pub struct Executable {
    pub dir: PathBuf,
    pub name: String,
    pub main: PathBuf,
    pub env: Env,
}

impl Executable {
    fn from_role_and_package(role: &Role, package: &Package) -> Executable {
        let mut env = package.env.clone();
        for (k, v) in role.env() {
            // only allow overrides of vars that already exist
            if env.contains_key(k) {
                env.insert(k.clone(), v.clone());
            }
        }

        Executable { dir: package.dir.clone(),
                     name: package.name.clone(),
                     main: package.main.clone(),
                     env: env }
    }
}

#[derive(Debug)]
pub struct PackageRepository {
    deps: HashMap<Package, Vec<Package>>,
}

impl PackageRepository {
    pub fn from_basedir<P: AsRef<Path>>(basedir: &P) -> Result<PackageRepository> {
        fn is_dir(entry: &DirEntry) -> bool {
            entry.file_type().is_dir()
        }

        let dir = match basedir.as_ref().canonicalize() {
            Ok(d) => d,
            Err(_) => { return Err(Error::new(PathBuf::from(basedir.as_ref()), "directory doesn't exist")) }
        };

        let mut deps = HashMap::new();
        let walker = WalkDir::new(&dir).max_depth(1).into_iter();

        for entry in walker.filter_entry(|e| is_dir(e)) {
            let entry = entry.unwrap();

            if let Ok(package) = Package::from_file(&dir, entry.path().file_name().unwrap().to_str().unwrap()) {
                let v = {
                    let package_dependencies = package.dependencies();
                    let mut v = Vec::with_capacity(package_dependencies.len());
                    for (dep_name, _) in package_dependencies {
                        // package_dependencies is a map of name => version
                        // but version is locked to "local" right now
                        v.push(try!(Package::from_file(&dir, dep_name)));
                    }
                    v
                };
                deps.insert(package, v);
            }
        };

        Ok(PackageRepository{deps: deps})
    }

    fn dependencies(&self, package: &Package) -> Option<&Vec<Package>> {
        self.deps.get(package).and_then(|d|
                                        if d.len() == 0 {
                                            None
                                        } else {
                                            Some(d)
                                        })
    }

    pub fn resolve(&self, roles: &Vec<Role>) -> Result<Vec<Executable>> {
        let mut marks: HashSet<Package> = HashSet::new();
        let mut temp_marks = HashSet::new();

        let mut sorted = Vec::with_capacity(roles.len());

        for role in roles {

            let mut stack = Stack::new();

            // reverse `packages` and add each to `stack`
            let mut packages_reversed = role.dependencies().clone();
            packages_reversed.reverse();
            for package in packages_reversed {
                stack.push(package);
            }

            loop {
                // Look for any saved packages at this level of the stack.
                // Add these to `sorted` immediately, because if they're here
                // then we know their deps have all been dealt with.
                loop {
                    if let Some(saved) = stack.pop_saved() {
                        marks.insert(saved.clone());
                        temp_marks.remove(&saved);
                        sorted.push(Executable::from_role_and_package(&role, &saved));
                    } else {
                        break;
                    }
                }

                // next, grab a package from the stack
                if let Some(package) = stack.pop() {

                    // if it's temp marked then there's a cycle
                    if temp_marks.contains(&package) {
                        return Err(Error::new(PathBuf::from("undefined"), "cycle detected"));
                    }

                    // if this package is already in `sorted` then ignore it, otherwise continue
                    if ! marks.contains(&package) {
                        temp_marks.insert(package.clone());

                        if let Some(deps) = self.dependencies(&package) {
                            // this package has dependencies.
                            // this means we need to save it to the side,
                            // indent the stack, and push all deps
                            stack.push_saved(package);
                            stack.indent();
                            for dep in deps {
                                stack.push(dep.clone());
                            }
                        } else {
                            // this package has no dependencies
                            // so we can immediately add it to `sorted`
                            marks.insert(package.clone());
                            temp_marks.remove(&package);
                            sorted.push(Executable::from_role_and_package(&role, &package));
                        }
                    }
                } else {
                    // there's nothing left on the stack at this level
                    // if we're not at the top level then outdent and look there
                    // if we're at the top then `sorted` is final
                    if ! stack.at_top() {
                        try!(stack.outdent());
                    } else {
                        // nothing left in stack
                        // nothing left in saved
                        // at the top level
                        // so we're done!
                        break;
                    }
                }
            }
        }
        Ok(sorted)
    }
}

#[cfg(test)]
mod tests {
    use super::{Stack, PackageRepository};
    use runlist::{Env, Package, Role};
    use std::path::{Path, PathBuf};

    fn resource<P: AsRef<Path>>(path: P) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("resources").join(path)
    }

    fn quick_role<P: AsRef<Path>>(path: P, name: String, dependencies: Vec<Package>, env: Env) -> Role {
        Role { path: path.as_ref().to_path_buf(),
               name: name,
               dependencies: dependencies,
               env: env }
    }

    #[test]
    fn stack_tests() {
        let mut s = Stack::new();
        assert!(s.at_top());
        assert_eq!(s.get().to_vec(), vec![]);
        assert_eq!(s.get_saved().to_vec(), vec![]);

        s.push(0);
        assert!(s.at_top());
        assert_eq!(s.get().to_vec(), vec![0]);

        s.push_saved(100);
        assert!(s.at_top());
        assert_eq!(s.get().to_vec(), vec![0]);
        assert_eq!(s.get_saved().to_vec(), vec![100]);

        s.push(1);
        s.push_saved(101);
        assert!(s.at_top());
        assert_eq!(s.get().to_vec(), vec![0, 1]);
        assert_eq!(s.get_saved().to_vec(), vec![101, 100]);

        s.indent();
        assert!(!s.at_top());
        assert_eq!(s.get().to_vec(), vec![]);
        assert_eq!(s.get_saved().to_vec(), vec![]);

        s.push(10);
        s.push(11);
        assert!(!s.at_top());
        assert_eq!(s.get().to_vec(), vec![10, 11]);
        assert_eq!(s.get_saved().to_vec(), vec![]);

        s.push_saved(200);
        s.push_saved(201);
        s.push_saved(202);
        assert!(!s.at_top());
        assert_eq!(s.get().to_vec(), vec![10, 11]);
        assert_eq!(s.get_saved().to_vec(), vec![202, 201, 200]);

        s.outdent().unwrap();
        assert!(s.at_top());
        assert_eq!(s.get().to_vec(), vec![0, 1]);
        assert_eq!(s.get_saved().to_vec(), vec![101, 100]);

        s.indent();
        assert!(!s.at_top());
        assert_eq!(s.get().to_vec(), vec![]);
        assert_eq!(s.get_saved().to_vec(), vec![]);

        s.push(0);
        s.push(1);
        s.push(2);
        s.push_saved(100);
        s.push_saved(101);
        s.push_saved(102);
        assert_eq!(s.get().to_vec(), vec![0, 1, 2]);
        assert_eq!(s.get_saved().to_vec(), vec![102, 101, 100]);
        assert_eq!(s.pop().unwrap(), 2);
        assert_eq!(s.pop_saved().unwrap(), 100);

        assert!(!s.at_top());
        assert!(s.outdent().is_ok());
        assert!(s.at_top());
        assert!(s.outdent().is_err());
    }

    #[test]
    fn simple_resolver_test() {
        let basedir = resource("package_repository_unit_tests").join("simple");
        let repo = PackageRepository::from_basedir(&basedir).unwrap();

        let a = Package::from_file(&basedir, "a").unwrap();
        let c = Package::from_file(&basedir, "c").unwrap();
        let z = Package::from_file(&basedir, "z").unwrap();

        let role = quick_role(&basedir, "foo".to_string(), vec![a.clone()], Env::new());
        assert_eq!(repo.resolve(&vec![role]).unwrap().iter().map(|exe| exe.name.as_str()).collect::<Vec<&str>>(),
                   vec!["d", "c", "b", "a"]);

        let role = quick_role(&basedir, "foo".to_string(), vec![a.clone(), z.clone()], Env::new());
        assert_eq!(repo.resolve(&vec![role]).unwrap().iter().map(|exe| exe.name.as_str()).collect::<Vec<&str>>(),
                   vec!["d", "c", "b", "a", "z"]);

        let role = quick_role(&basedir, "foo".to_string(), vec![a.clone(), c.clone()], Env::new());
        assert_eq!(repo.resolve(&vec![role]).unwrap().iter().map(|exe| exe.name.as_str()).collect::<Vec<&str>>(),
                   vec!["d", "c", "b", "a"]);
    }

    #[test]
    fn cyclical_resolver_test() {
        let basedir = resource("package_repository_unit_tests").join("cycle");
        let repo = PackageRepository::from_basedir(&basedir).unwrap();

        let a = Package::from_file(&basedir, "a").unwrap();

        let role = quick_role(&basedir, "foo".to_string(), vec![a.clone()], Env::new());
        let r = repo.resolve(&vec![role]);
        assert!(r.is_err());
    }

    #[test]
    fn more_complicated_resolver_test() {
        let basedir = resource("package_repository_unit_tests").join("more_complicated");
        let repo = PackageRepository::from_basedir(&basedir).unwrap();

        let a = Package::from_file(&basedir, "a").unwrap();

        let role = quick_role(&basedir, "foo".to_string(), vec![a.clone()], Env::new());
        assert_eq!(repo.resolve(&vec![role]).unwrap().iter().map(|exe| exe.name.as_str()).collect::<Vec<&str>>(),
                   vec!["d", "c", "b", "c1", "a"]);
    }

    #[test]
    fn env_test() {
        let basedir = resource("package_repository_unit_tests").join("env");
        let repo = PackageRepository::from_basedir(&basedir).unwrap();

        let has_env = Package::from_file(&basedir, "has_env").unwrap();
        let no_env = Package::from_file(&basedir, "no_env").unwrap();

        // no overrides for a package with an env
        let role_env = Env::new();
        let role = quick_role(&basedir, "foo".to_string(), vec![has_env.clone()], role_env);
        let mut expected_env = Env::new();
        expected_env.insert("FOO".to_string(), "foo from package".to_string());
        expected_env.insert("BAR".to_string(), "bar from package".to_string());
        let exes = repo.resolve(&vec![role]).unwrap();
        assert_eq!(exes.len(), 1);
        assert_eq!(exes[0].env, expected_env);

        // role override for a package with an env
        let mut role_env = Env::new();
        role_env.insert("FOO".to_string(), "foo from role".to_string());
        let role = quick_role(&basedir, "foo".to_string(), vec![has_env.clone()], role_env);
        let mut expected_env = Env::new();
        expected_env.insert("FOO".to_string(), "foo from role".to_string());
        expected_env.insert("BAR".to_string(), "bar from package".to_string());
        let exes = repo.resolve(&vec![role]).unwrap();
        assert_eq!(exes.len(), 1);
        assert_eq!(exes[0].env, expected_env);

        // no overrides for a package with no env
        let role_env = Env::new();
        let role = quick_role(&basedir, "foo".to_string(), vec![no_env.clone()], role_env);
        let expected_env = Env::new();
        let exes = repo.resolve(&vec![role]).unwrap();
        assert_eq!(exes.len(), 1);
        assert_eq!(exes[0].env, expected_env);

        // role override for a package with no env
        let mut role_env = Env::new();
        role_env.insert("FOO".to_string(), "foo from role".to_string());
        let role = quick_role(&basedir, "foo".to_string(), vec![no_env.clone()], role_env);
        let expected_env = Env::new();
        let exes = repo.resolve(&vec![role]).unwrap();
        assert_eq!(exes.len(), 1);
        assert_eq!(exes[0].env, expected_env);


    }
}
