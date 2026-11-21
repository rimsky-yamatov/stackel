//! Module and package resolution.
//!
//! A program is made of modules; one file is one module. The module that is run is
//! the *entry* module. `use a.b (x, y)` imports names of the module `a.b`, and
//! `use a.b` imports all its public names. Whatever is imported, its public names can
//! also be written qualified (`a.b.x`).
//!
//! The path of a `use` is resolved in this order: a dependency package declared in
//! `stackel.toml` (`use json.decode` is the module `decode` of the package `json`, and
//! `use json` is its root module `src/lib.stk`), then the standard library (`std.*`), and
//! finally a module of the current package (`src/a/b.stk`).
//!
//! The resolver loads every module, checks visibility (`pub`) and rewrites the names so
//! that the rest of the pipeline sees one flat program: the word `x` of module `a.b`
//! becomes `a.b.x`, while the entry module keeps its names. While a program is checked
//! and run, the position of a token in the file number `n` is stored as
//! `line + (n << FILE_SHIFT)`, so that errors can tell which file they belong to; `localize`
//! turns such a position back into a file, a line and a column.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::ast::{Arm, CtorDef, Def, Node, Origin, Program, Sig, TypeDef, Use};
use crate::builtins;
use crate::error::{Error, Result};
use crate::lexer::{self, Tok};
use crate::manifest::{self, Manifest};
use crate::parser;
use crate::source::{normalize, Fs, MemFs};
use crate::stdlib;
use crate::types::KNOWN_EFFECTS;

pub const FILE_SHIFT: u32 = 20;

/// A program whose modules have been loaded and resolved.
#[derive(Debug)]
pub struct Loaded {
    pub program: Program,
    /// The names of the source files; the entry file is first. An empty name means "no name".
    pub files: Vec<String>,
    /// The texts of the source files, in the same order.
    pub sources: Vec<String>,
}

/// The source line with a mark under the column, in the style of compiler messages.
fn source_snippet(text: &str, line: usize, col: usize) -> Option<String> {
    let source_line = text.lines().nth(line.checked_sub(1)?)?;
    let number = line.to_string();
    let gutter = " ".repeat(number.len());
    let spaces = " ".repeat(col.saturating_sub(1));
    Some(format!(
        "{g} |\n{n} | {l}\n{g} | {s}^",
        g = gutter,
        n = number,
        l = source_line,
        s = spaces
    ))
}

/// Turns an error with a tagged position into an error with a file, a line and a column, and
/// with the source line it points to.
pub fn localize(files: &[String], sources: &[String], e: Error) -> Error {
    if e.line == 0 || e.file.is_some() {
        return e;
    }
    let id = e.line >> FILE_SHIFT;
    let line = e.line & ((1usize << FILE_SHIFT) - 1);
    let file = files.get(id).filter(|name| !name.is_empty()).cloned();
    let snippet = sources
        .get(id)
        .and_then(|text| source_snippet(text, line, e.col));
    let mut out = Error::new(e.msg, line, e.col);
    out.file = file;
    out.snippet = snippet;
    out
}

fn shift_error(e: Error, off: usize) -> Error {
    if e.line == 0 {
        e
    } else {
        Error::new(e.msg, e.line + off, e.col)
    }
}

/// Loads the program that starts at `entry`: a `.stk` file, or a package directory
/// (which has a `stackel.toml`).
pub fn load(fs: &dyn Fs, entry: &Path) -> Result<Loaded> {
    load_with(fs, entry, false)
}

/// Loads a program from a single source text. Only the standard library can be imported.
pub fn load_source(src: &str) -> Result<Loaded> {
    let fs = MemFs::new().with("main.stk", src);
    load_with(&fs, Path::new("main.stk"), true)
}

fn load_with(fs: &dyn Fs, entry: &Path, hide_entry_name: bool) -> Result<Loaded> {
    let mut loader = Loader::new(fs, hide_entry_name);
    if let Err(e) = loader.build(entry) {
        return Err(localize(&loader.files, &loader.sources, e));
    }
    loader.finish()
}

struct Package {
    /// For the root package: the name in its manifest, if it has one.
    name: String,
    root: PathBuf,
    /// The directory that holds the modules.
    src: PathBuf,
    /// Dependency name -> package.
    deps: HashMap<String, usize>,
    /// The effects the package may perform (`allow`); `None` means no limit.
    allow: Option<BTreeSet<String>>,
}

#[derive(Default, Clone)]
struct Names {
    /// Words and constructors: the name in the module -> the global name.
    words: HashMap<String, String>,
    types: HashMap<String, String>,
}

enum Target {
    Std(String),
    Module(usize),
}

struct Module {
    /// The prefix of the global names: empty for the entry module.
    key: String,
    /// Index into `Loader::files`.
    file: usize,
    package: usize,
    program: Program,
    targets: Vec<(Use, Target)>,
    done: bool,
}

/// The global name of `name` in the module with the given key.
fn global_name(key: &str, name: &str) -> String {
    if key.is_empty() {
        name.to_string()
    } else {
        format!("{}.{}", key, name)
    }
}

/// `a/b.stk` -> `a.b`
fn module_path_of(rel: &Path) -> String {
    rel.with_extension("")
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<String>>()
        .join(".")
}

/// Whether a word of a type signature names a type (and not a type variable).
fn is_type_name(w: &str) -> bool {
    if matches!(w, "Int" | "Float" | "String" | "Bool") {
        return false;
    }
    let last = w.rsplit('.').next().unwrap_or(w);
    last.chars().next().map_or(false, |c| c.is_uppercase())
}

struct Loader<'f> {
    fs: &'f dyn Fs,
    packages: Vec<Package>,
    modules: Vec<Module>,
    /// (package, module path inside the package) -> module. The root module of a
    /// dependency has the path "".
    index: HashMap<(usize, String), usize>,
    files: Vec<String>,
    /// The texts of the files.
    sources: Vec<String>,
    /// The modules in the order they were completed: a module comes after the modules it uses.
    order: Vec<usize>,
    /// The modules that are being loaded right now (to find import cycles).
    stack: Vec<usize>,
    entry: usize,
    hide_entry_name: bool,
    /// The pseudo package of the embedded standard library modules, once one is loaded.
    std_package: Option<usize>,
}

impl<'f> Loader<'f> {
    fn new(fs: &'f dyn Fs, hide_entry_name: bool) -> Self {
        Loader {
            fs,
            packages: Vec::new(),
            modules: Vec::new(),
            index: HashMap::new(),
            files: Vec::new(),
            sources: Vec::new(),
            order: Vec::new(),
            stack: Vec::new(),
            entry: 0,
            hide_entry_name,
            std_package: None,
        }
    }

    /// An error at `line` of the file of the module `module`.
    fn at(&self, module: usize, line: usize, msg: impl Into<String>) -> Error {
        Error::new(msg, line + (self.modules[module].file << FILE_SHIFT), 1)
    }

    fn read_manifest(&self, root: &Path) -> Result<Option<Manifest>> {
        let path = root.join("stackel.toml");
        if !self.fs.is_file(&path) {
            return Ok(None);
        }
        let text = self
            .fs
            .read_to_string(&path)
            .map_err(|e| Error::runtime(format!("cannot read {}: {}", path.display(), e)))?;
        manifest::parse_manifest(&text)
            .map(Some)
            .map_err(|e| e.in_file(path.display().to_string()))
    }

    /// Finds the root package and loads the entry module and everything it uses.
    fn build(&mut self, entry: &Path) -> Result<()> {
        let entry = normalize(entry);
        let (root_dir, manifest, entry_file) = if self.fs.is_dir(&entry) {
            let manifest = match self.read_manifest(&entry)? {
                Some(m) => m,
                None => {
                    return Err(Error::runtime(format!(
                        "`{}` has no stackel.toml, so it is not a package",
                        entry.display()
                    )))
                }
            };
            let rel = manifest
                .entry
                .clone()
                .unwrap_or_else(|| "src/main.stk".to_string());
            let file = normalize(&entry.join(rel));
            (entry.clone(), Some(manifest), file)
        } else {
            // A file belongs to the package of the nearest manifest above it, if there is one.
            let mut found: Option<(PathBuf, Manifest)> = None;
            for dir in entry.ancestors().skip(1) {
                if self.fs.is_file(&dir.join("stackel.toml")) {
                    if let Some(m) = self.read_manifest(dir)? {
                        found = Some((dir.to_path_buf(), m));
                    }
                    break;
                }
            }
            match found {
                Some((dir, m)) => (dir, Some(m), entry.clone()),
                None => {
                    let dir = entry.parent().map(|p| p.to_path_buf()).unwrap_or_default();
                    (dir, None, entry.clone())
                }
            }
        };

        let src = if manifest.is_some() {
            root_dir.join("src")
        } else {
            root_dir.clone()
        };
        let name = manifest
            .as_ref()
            .map(|m| m.name.clone())
            .unwrap_or_default();
        self.packages.push(Package {
            name,
            root: root_dir,
            src: src.clone(),
            deps: HashMap::new(),
            allow: None,
        });
        if let Some(m) = &manifest {
            self.add_deps(0, m)?;
        }

        // The entry file is a module of the root package if it lies inside `src`.
        let rel = match entry_file.strip_prefix(&src) {
            Ok(r) => Some(module_path_of(r)),
            Err(_) => None,
        };
        self.entry = self.load_module(0, rel, entry_file, String::new())?;
        Ok(())
    }

    /// Loads the packages that the manifest of package `pkg` depends on.
    fn add_deps(&mut self, pkg: usize, manifest: &Manifest) -> Result<()> {
        let root = self.packages[pkg].root.clone();
        let mpath = root.join("stackel.toml").display().to_string();
        for (name, spec) in &manifest.deps {
            let fail = |msg: String| Error::new(msg, spec.line.max(1), 1).in_file(mpath.clone());
            let rel = match &spec.path {
                Some(p) => p.clone(),
                None => {
                    return Err(fail(format!(
                        "dependency `{}` needs a `path` (registry dependencies are not supported yet)",
                        name
                    )))
                }
            };
            if let Some(allow) = &spec.allow {
                for e in allow {
                    if !KNOWN_EFFECTS.contains(&e.as_str()) {
                        return Err(fail(format!(
                            "unknown effect `{}` in `allow` of dependency `{}` (known effects: {})",
                            e,
                            name,
                            KNOWN_EFFECTS.join(", ")
                        )));
                    }
                }
            }
            let allow: Option<BTreeSet<String>> =
                spec.allow.as_ref().map(|a| a.iter().cloned().collect());
            let dep_root = normalize(&root.join(&rel));
            let idx = self.add_package(dep_root, name, allow, &mpath, spec.line)?;
            self.packages[pkg].deps.insert(name.clone(), idx);
        }
        Ok(())
    }

    /// Loads the dependency `name` that lives in `root`. `from` and `line` locate the
    /// declaration in the manifest of the package that needs it.
    fn add_package(
        &mut self,
        root: PathBuf,
        name: &str,
        allow: Option<BTreeSet<String>>,
        from: &str,
        line: usize,
    ) -> Result<usize> {
        let fail = |msg: String| Error::new(msg, line.max(1), 1).in_file(from.to_string());

        if let Some(i) = self.packages.iter().position(|p| p.root == root) {
            if i == 0 {
                return Err(fail(format!(
                    "dependency `{}` is the root package itself (a package cannot depend on itself)",
                    name
                )));
            }
            if self.packages[i].name != name {
                return Err(fail(format!(
                    "the package in `{}` is already used under the name `{}`, not `{}`",
                    root.display(),
                    self.packages[i].name,
                    name
                )));
            }
            // Every package that needs it may limit its effects; the limits add up.
            let merged = match (self.packages[i].allow.take(), allow) {
                (None, None) => None,
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (Some(a), Some(b)) => Some(a.intersection(&b).cloned().collect::<BTreeSet<String>>()),
            };
            self.packages[i].allow = merged;
            return Ok(i);
        }

        let manifest = match self.read_manifest(&root)? {
            Some(m) => m,
            None => {
                return Err(fail(format!(
                    "dependency `{}`: `{}` has no stackel.toml",
                    name,
                    root.display()
                )))
            }
        };
        if manifest.name != name {
            return Err(fail(format!(
                "dependency `{}` points to a package named `{}`; a dependency is named like its package",
                name, manifest.name
            )));
        }
        if self.packages.iter().any(|p| p.name == name) {
            return Err(fail(format!(
                "two different packages are named `{}`",
                name
            )));
        }
        self.packages.push(Package {
            name: name.to_string(),
            root: root.clone(),
            src: root.join("src"),
            deps: HashMap::new(),
            allow,
        });
        let idx = self.packages.len() - 1;
        self.add_deps(idx, &manifest)?;
        Ok(idx)
    }

    fn module_file(&self, pkg: usize, rel: &str) -> PathBuf {
        let src = &self.packages[pkg].src;
        if rel.is_empty() {
            src.join("lib.stk")
        } else {
            src.join(format!("{}.stk", rel.replace('.', "/")))
        }
    }

    fn module_key(&self, pkg: usize, rel: &str) -> String {
        if pkg == 0 {
            rel.to_string()
        } else if rel.is_empty() {
            self.packages[pkg].name.clone()
        } else {
            format!("{}.{}", self.packages[pkg].name, rel)
        }
    }

    /// Reads and parses a module, and loads the modules it uses.
    fn load_module(
        &mut self,
        pkg: usize,
        rel: Option<String>,
        path: PathBuf,
        key: String,
    ) -> Result<usize> {
        let text = self
            .fs
            .read_to_string(&path)
            .map_err(|e| Error::runtime(format!("cannot read {}: {}", path.display(), e)))?;
        let name = if self.files.is_empty() && self.hide_entry_name {
            String::new()
        } else {
            path.display().to_string()
        };
        self.load_text(pkg, rel, name, text, key)
    }

    /// Parses the text of a module and loads the modules it uses.
    fn load_text(
        &mut self,
        pkg: usize,
        rel: Option<String>,
        name: String,
        text: String,
        key: String,
    ) -> Result<usize> {
        let file = self.files.len();
        self.files.push(name);
        self.sources.push(text.clone());
        let off = file << FILE_SHIFT;
        let tokens = lexer::lex(&text).map_err(|e| shift_error(e, off))?;
        let program = parser::parse(&tokens).map_err(|e| shift_error(e, off))?;

        let idx = self.modules.len();
        let uses = program.uses.clone();
        self.modules.push(Module {
            key,
            file,
            package: pkg,
            program,
            targets: Vec::new(),
            done: false,
        });
        if let Some(r) = rel {
            self.index.insert((pkg, r), idx);
        }

        self.stack.push(idx);
        let mut targets = Vec::with_capacity(uses.len());
        for u in uses {
            let target = self.resolve_use(idx, &u)?;
            targets.push((u, target));
        }
        self.stack.pop();
        self.modules[idx].targets = targets;
        self.modules[idx].done = true;
        self.order.push(idx);
        Ok(idx)
    }

    /// Finds the module a `use` refers to, and loads it if that has not happened yet.
    fn resolve_use(&mut self, from: usize, u: &Use) -> Result<Target> {
        if u.source.is_some() {
            return Err(self.at(
                from,
                u.line,
                "`use ... from` is not supported yet; declare the dependency in stackel.toml",
            ));
        }
        if u.path.split('.').any(|s| s.is_empty()) {
            return Err(self.at(from, u.line, format!("invalid module path `{}`", u.path)));
        }
        let pkg = self.modules[from].package;
        let first = u.path.split('.').next().unwrap_or("").to_string();
        let (target_pkg, rel) = if let Some(&dep) = self.packages[pkg].deps.get(&first) {
            let rest = u.path[first.len()..].trim_start_matches('.').to_string();
            (dep, rest)
        } else if first == "std" {
            if let Some(text) = stdlib::source(&u.path) {
                return self.resolve_embedded(from, u, text);
            }
            return match stdlib::module(&u.path) {
                Some(_) => Ok(Target::Std(u.path.clone())),
                None => Err(self.at(
                    from,
                    u.line,
                    format!("unknown standard library module `{}`", u.path),
                )),
            };
        } else {
            (pkg, u.path.clone())
        };
        // `lib` is the root module of a dependency.
        let rel = if target_pkg != 0 && rel == "lib" {
            String::new()
        } else {
            rel
        };

        if let Some(&m) = self.index.get(&(target_pkg, rel.clone())) {
            if !self.modules[m].done {
                return Err(self.cycle(from, m, u.line));
            }
            return Ok(Target::Module(m));
        }
        let path = self.module_file(target_pkg, &rel);
        if !self.fs.is_file(&path) {
            return Err(self.at(
                from,
                u.line,
                format!(
                    "module `{}` not found (looked for `{}`)",
                    u.path,
                    path.display()
                ),
            ));
        }
        let key = self.module_key(target_pkg, &rel);
        let m = self.load_module(target_pkg, Some(rel), path, key)?;
        Ok(Target::Module(m))
    }

    /// Loads a standard module that is written in Stackel (and embedded in the program),
    /// unless it has been loaded already.
    fn resolve_embedded(&mut self, from: usize, u: &Use, text: &str) -> Result<Target> {
        let pkg = match self.std_package {
            Some(p) => p,
            None => {
                self.packages.push(Package {
                    name: "std".to_string(),
                    root: PathBuf::new(),
                    src: PathBuf::new(),
                    deps: HashMap::new(),
                    allow: None,
                });
                let p = self.packages.len() - 1;
                self.std_package = Some(p);
                p
            }
        };
        if let Some(&m) = self.index.get(&(pkg, u.path.clone())) {
            if !self.modules[m].done {
                return Err(self.cycle(from, m, u.line));
            }
            return Ok(Target::Module(m));
        }
        let name = format!("<{}>", u.path);
        let m = self.load_text(
            pkg,
            Some(u.path.clone()),
            name,
            text.to_string(),
            u.path.clone(),
        )?;
        Ok(Target::Module(m))
    }

    fn cycle(&self, from: usize, target: usize, line: usize) -> Error {
        let show = |i: usize| {
            let name = &self.files[self.modules[i].file];
            if name.is_empty() {
                "<input>".to_string()
            } else {
                name.clone()
            }
        };
        let start = self.stack.iter().position(|&i| i == target).unwrap_or(0);
        let mut names: Vec<String> = self.stack[start..].iter().map(|&i| show(i)).collect();
        names.push(show(target));
        self.at(
            from,
            line,
            format!("circular import: {}", names.join(" -> ")),
        )
    }

    fn finish(self) -> Result<Loaded> {
        let files = self.files.clone();
        let sources = self.sources.clone();
        match self.assemble() {
            Ok(program) => Ok(Loaded {
                program,
                files,
                sources,
            }),
            Err(e) => Err(localize(&files, &sources, e)),
        }
    }

    fn check_own_name(&self, m: usize, name: &str, what: &str, line: usize) -> Result<()> {
        if name.contains('.') {
            return Err(self.at(
                m,
                line,
                format!("the name of a {} cannot contain `.`: `{}`", what, name),
            ));
        }
        Ok(())
    }

    /// The names a module defines, and the part of them that other modules may import.
    fn compute_names(&self, m: usize) -> Result<(Names, Names)> {
        let module = &self.modules[m];
        let key = &module.key;
        let is_entry = m == self.entry;
        let mut own = Names::default();
        let mut public = Names::default();

        // A word is public if its definition or its signature says `pub`.
        let mut pub_words: HashSet<&str> = HashSet::new();
        for d in &module.program.defs {
            if d.public {
                pub_words.insert(d.name.as_str());
            }
        }
        for s in &module.program.sigs {
            if s.public {
                pub_words.insert(s.name.as_str());
            }
        }

        for d in &module.program.defs {
            self.check_own_name(m, &d.name, "word", d.line)?;
            if builtins::is_builtin(&d.name) {
                return Err(self.at(
                    m,
                    d.line,
                    format!("cannot redefine the builtin word `{}`", d.name),
                ));
            }
            let global = global_name(key, &d.name);
            own.words.insert(d.name.clone(), global.clone());
            if pub_words.contains(d.name.as_str()) {
                if !is_entry && !module.program.sigs.iter().any(|s| s.name == d.name) {
                    return Err(self.at(
                        m,
                        d.line,
                        format!("public word `{}` needs a type signature", d.name),
                    ));
                }
                public.words.insert(d.name.clone(), global);
            }
        }

        for t in &module.program.types {
            self.check_own_name(m, &t.name, "type", t.line)?;
            if matches!(t.name.as_str(), "Int" | "Float" | "String" | "Bool") {
                return Err(self.at(
                    m,
                    t.line,
                    format!("`{}` is a builtin type and cannot be redefined", t.name),
                ));
            }
            let global = global_name(key, &t.name);
            own.types.insert(t.name.clone(), global.clone());
            if t.public {
                public.types.insert(t.name.clone(), global);
            }
            for c in &t.ctors {
                self.check_own_name(m, &c.name, "constructor", c.line)?;
                let g = global_name(key, &c.name);
                own.words.insert(c.name.clone(), g.clone());
                if t.public {
                    public.words.insert(c.name.clone(), g);
                }
            }
        }
        Ok((own, public))
    }

    fn import_explicit(
        &self,
        m: usize,
        layers: &mut Layers,
        name: &str,
        global: &str,
        u: &Use,
    ) -> Result<()> {
        if layers.own.contains_key(name) {
            return Err(self.at(
                m,
                u.line,
                format!(
                    "`{}` is imported from `{}`, but this module also defines it",
                    name, u.path
                ),
            ));
        }
        if let Some(existing) = layers.explicit.get(name) {
            if existing.as_str() != global {
                return Err(self.at(
                    m,
                    u.line,
                    format!("`{}` is imported twice, from different modules", name),
                ));
            }
        }
        layers
            .explicit
            .insert(name.to_string(), global.to_string());
        Ok(())
    }

    /// What the names mean inside module `m`. `names[i]` holds the own and the public names
    /// of module `i`.
    fn build_scope(&self, m: usize, names: &[(Names, Names)]) -> Result<Scope> {
        let module = &self.modules[m];
        let mut scope = Scope::default();
        scope.words.own = names[m].0.words.clone();
        scope.types.own = names[m].0.types.clone();

        for (u, target) in &module.targets {
            match target {
                Target::Std(name) => {
                    let words = stdlib::module(name)
                        .expect("the module was checked when the use was resolved");
                    for w in words.iter() {
                        scope
                            .qual_words
                            .insert(format!("{}.{}", u.path, w), w.to_string());
                    }
                    for n in &u.names {
                        if !words.contains(&n.as_str()) {
                            return Err(self.at(
                                m,
                                u.line,
                                format!("module `{}` has no word `{}`", u.path, n),
                            ));
                        }
                    }
                }
                Target::Module(t) => {
                    let own = &names[*t].0;
                    let public = &names[*t].1;
                    for (n, g) in &public.words {
                        scope
                            .qual_words
                            .insert(format!("{}.{}", u.path, n), g.clone());
                    }
                    for (n, g) in &public.types {
                        scope
                            .qual_types
                            .insert(format!("{}.{}", u.path, n), g.clone());
                    }
                    if u.names.is_empty() {
                        for (n, g) in &public.words {
                            scope.words.add_wild(n, g);
                        }
                        for (n, g) in &public.types {
                            scope.types.add_wild(n, g);
                        }
                    } else {
                        for n in &u.names {
                            let word = public.words.get(n);
                            let ty = public.types.get(n);
                            if word.is_none() && ty.is_none() {
                                let msg = if own.words.contains_key(n) || own.types.contains_key(n)
                                {
                                    format!(
                                        "`{}` is private in module `{}`; mark it `pub` to export it",
                                        n, u.path
                                    )
                                } else {
                                    format!("module `{}` has no public name `{}`", u.path, n)
                                };
                                return Err(self.at(m, u.line, msg));
                            }
                            if let Some(g) = word {
                                self.import_explicit(m, &mut scope.words, n, g, u)?;
                            }
                            if let Some(g) = ty {
                                self.import_explicit(m, &mut scope.types, n, g, u)?;
                            }
                        }
                    }
                }
            }
        }
        Ok(scope)
    }

    /// Rewrites the types, signatures and words of module `m` to global names.
    fn rewrite_module(&self, m: usize, scope: &Scope) -> Result<(Vec<TypeDef>, Vec<Sig>, Vec<Def>)> {
        let module = &self.modules[m];
        let off = module.file << FILE_SHIFT;
        let key = &module.key;
        let rw = Rewriter { scope, off };

        let mut types = Vec::new();
        for t in &module.program.types {
            let mut ctors = Vec::new();
            for c in &t.ctors {
                ctors.push(CtorDef {
                    name: global_name(key, &c.name),
                    fields: rw.type_tokens(&c.fields, c.line)?,
                    line: c.line + off,
                });
            }
            types.push(TypeDef {
                name: global_name(key, &t.name),
                public: t.public,
                params: t.params.clone(),
                ctors,
                line: t.line + off,
            });
        }
        let mut sigs = Vec::new();
        for s in &module.program.sigs {
            sigs.push(Sig {
                name: global_name(key, &s.name),
                public: s.public,
                tokens: rw.type_tokens(&s.tokens, s.line)?,
                line: s.line + off,
            });
        }
        let mut defs = Vec::new();
        for d in &module.program.defs {
            defs.push(Def {
                name: global_name(key, &d.name),
                public: d.public,
                body: Rc::new(rw.nodes(&d.body, &[])?),
                line: d.line + off,
            });
        }
        Ok((types, sigs, defs))
    }

    /// Resolves the names of all modules and joins them into one program.
    fn assemble(&self) -> Result<Program> {
        let mut names: Vec<(Names, Names)> = Vec::with_capacity(self.modules.len());
        for m in 0..self.modules.len() {
            names.push(self.compute_names(m)?);
        }

        let mut program = Program::default();
        program.module = self.modules[self.entry].program.module.clone();
        for &m in &self.order {
            let scope = self.build_scope(m, &names)?;
            let (types, sigs, defs) = self.rewrite_module(m, &scope)?;
            let package = &self.packages[self.modules[m].package];
            if let Some(allow) = &package.allow {
                let list: Vec<String> = allow.iter().cloned().collect();
                for d in &defs {
                    program.origins.insert(
                        d.name.clone(),
                        Origin {
                            package: package.name.clone(),
                            allow: list.clone(),
                        },
                    );
                }
            }
            program.types.extend(types);
            program.sigs.extend(sigs);
            program.defs.extend(defs);
        }
        Ok(program)
    }
}

/// The meaning of the names of one namespace inside a module. A module's own names win over
/// explicitly imported ones, which win over the ones brought in by `use module`.
#[derive(Default)]
struct Layers {
    own: HashMap<String, String>,
    explicit: HashMap<String, String>,
    /// name -> the distinct global names it may stand for
    wild: HashMap<String, Vec<String>>,
}

enum Found {
    One(String),
    Many(Vec<String>),
    Missing,
}

impl Layers {
    fn lookup(&self, name: &str) -> Found {
        if let Some(g) = self.own.get(name) {
            return Found::One(g.clone());
        }
        if let Some(g) = self.explicit.get(name) {
            return Found::One(g.clone());
        }
        match self.wild.get(name) {
            Some(gs) if gs.len() == 1 => Found::One(gs[0].clone()),
            Some(gs) if gs.len() > 1 => Found::Many(gs.clone()),
            _ => Found::Missing,
        }
    }

    fn add_wild(&mut self, name: &str, global: &str) {
        let entry = self.wild.entry(name.to_string()).or_default();
        if !entry.iter().any(|g| g.as_str() == global) {
            entry.push(global.to_string());
        }
    }
}

#[derive(Default)]
struct Scope {
    words: Layers,
    types: Layers,
    /// `a.b.x` -> global name, for every `use a.b`
    qual_words: HashMap<String, String>,
    qual_types: HashMap<String, String>,
}

struct Rewriter<'a> {
    scope: &'a Scope,
    /// `file << FILE_SHIFT`
    off: usize,
}

impl<'a> Rewriter<'a> {
    fn settle(name: &str, found: Found) -> std::result::Result<String, String> {
        match found {
            Found::One(g) => Ok(g),
            // Unknown names stay as they are: they are builtins, or the checker reports them.
            Found::Missing => Ok(name.to_string()),
            Found::Many(gs) => Err(format!(
                "`{}` is ambiguous: it is imported from several modules ({}); use a qualified name",
                name,
                gs.join(", ")
            )),
        }
    }

    /// A word or a constructor.
    fn value(&self, name: &str) -> std::result::Result<String, String> {
        let found = if name.contains('.') {
            match self.scope.qual_words.get(name) {
                Some(g) => Found::One(g.clone()),
                None => Found::Missing,
            }
        } else {
            self.scope.words.lookup(name)
        };
        Self::settle(name, found)
    }

    fn type_name(&self, name: &str) -> std::result::Result<String, String> {
        let found = if name.contains('.') {
            match self.scope.qual_types.get(name) {
                Some(g) => Found::One(g.clone()),
                None => Found::Missing,
            }
        } else {
            self.scope.types.lookup(name)
        };
        Self::settle(name, found)
    }

    /// Rewrites the names in a body. `locals` are the names bound by enclosing `let`s: they hide
    /// words, and they are never rewritten.
    fn nodes(&self, nodes: &[Node], locals: &[String]) -> Result<Vec<Node>> {
        let mut out = Vec::with_capacity(nodes.len());
        for n in nodes {
            let node = match n {
                Node::Int(v) => Node::Int(*v),
                Node::Float(v) => Node::Float(*v),
                Node::Str(s) => Node::Str(s.clone()),
                Node::Quote(q) => Node::Quote(Rc::new(self.nodes(q, locals)?)),
                Node::Word { name, line, col } => {
                    let new = if locals.contains(name) {
                        name.clone()
                    } else {
                        self.value(name)
                            .map_err(|m| Error::new(m, line + self.off, *col))?
                    };
                    Node::Word {
                        name: new,
                        line: line + self.off,
                        col: *col,
                    }
                }
                Node::If {
                    then_branch,
                    else_branch,
                    line,
                    col,
                } => Node::If {
                    then_branch: Rc::new(self.nodes(then_branch, locals)?),
                    else_branch: Rc::new(self.nodes(else_branch, locals)?),
                    line: line + self.off,
                    col: *col,
                },
                Node::Match { arms, line, col } => {
                    let mut new_arms = Vec::with_capacity(arms.len());
                    for a in arms {
                        let ctor = self
                            .value(&a.ctor)
                            .map_err(|m| Error::new(m, a.line + self.off, a.col))?;
                        new_arms.push(Arm {
                            ctor,
                            body: Rc::new(self.nodes(&a.body, locals)?),
                            line: a.line + self.off,
                            col: a.col,
                        });
                    }
                    Node::Match {
                        arms: new_arms,
                        line: line + self.off,
                        col: *col,
                    }
                }
                Node::Let {
                    names,
                    body,
                    line,
                    col,
                } => {
                    let mut inner: Vec<String> = locals.to_vec();
                    inner.extend(names.iter().cloned());
                    Node::Let {
                        names: names.clone(),
                        body: Rc::new(self.nodes(body, &inner)?),
                        line: line + self.off,
                        col: *col,
                    }
                }
            };
            out.push(node);
        }
        Ok(out)
    }

    /// Rewrites the type names in the tokens of a signature or of the fields of a constructor.
    /// Type variables, builtin types and effect names are left alone.
    fn type_tokens(&self, toks: &[Tok], line: usize) -> Result<Vec<Tok>> {
        let mut out = Vec::with_capacity(toks.len());
        let mut in_effects = false;
        for t in toks {
            match t {
                Tok::Word(w) if w == "!" => {
                    in_effects = true;
                    out.push(t.clone());
                }
                Tok::RBracket => {
                    in_effects = false;
                    out.push(t.clone());
                }
                Tok::Word(w) if !in_effects && is_type_name(w) => {
                    let new = self
                        .type_name(w)
                        .map_err(|m| Error::new(m, line + self.off, 1))?;
                    out.push(Tok::Word(new));
                }
                other => out.push(other.clone()),
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localize_decodes_the_file_and_the_line() {
        let files = vec!["main.stk".to_string(), "util.stk".to_string()];
        let sources = vec!["a\nb\n".to_string(), "1\n2\n3\n4\nfive six\n".to_string()];
        let e = localize(&files, &sources, Error::new("boom", 5 + (1 << FILE_SHIFT), 3));
        assert_eq!(e.snippet.as_deref(), Some("  |\n5 | five six\n  |   ^"));
        assert_eq!((e.line, e.col), (5, 3));
        assert_eq!(e.file.as_deref(), Some("util.stk"));
        assert_eq!(e.to_string(), "error in util.stk at 5:3: boom");

        let e = localize(&files, &sources, Error::new("boom", 7, 1));
        assert_eq!(e.file.as_deref(), Some("main.stk"));
        // a line that the text does not have has no snippet
        assert_eq!(e.snippet, None);

        // an unnamed file stays unnamed
        let e = localize(&["".to_string()], &[], Error::new("boom", 7, 1));
        assert_eq!(e.file, None);
        assert_eq!(e.to_string(), "error at 7:1: boom");

        // run-time errors without a position are left alone
        let e = localize(&files, &sources, Error::runtime("boom"));
        assert_eq!(e.line, 0);
        assert_eq!(e.file, None);
    }

    #[test]
    fn module_paths_come_from_file_paths() {
        assert_eq!(module_path_of(Path::new("a/b.stk")), "a.b");
        assert_eq!(module_path_of(Path::new("main.stk")), "main");
    }

    #[test]
    fn type_names_are_told_from_type_variables() {
        assert!(is_type_name("Option"));
        assert!(is_type_name("opt.Option"));
        assert!(!is_type_name("a"));
        assert!(!is_type_name("..r"));
        assert!(!is_type_name("Int"));
        assert!(!is_type_name("String"));
    }

    #[test]
    fn global_names() {
        assert_eq!(global_name("", "x"), "x");
        assert_eq!(global_name("a.b", "x"), "a.b.x");
    }
}
