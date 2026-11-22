//! The package manifest `stackel.toml` (a small subset of TOML).
//!
//! ```toml
//! [package]
//! name = "app"
//! version = "0.1.0"
//! entry = "src/main.stk"            # optional; this is the default
//!
//! [deps]
//! mathx = { path = "../mathx", allow = [] }
//! json = { path = "../json", allow = ["IO"] }
//!
//! [deps.parser]                     # the same, as a table of its own
//! path = "../parser"
//! ```
//!
//! Supported values: strings, lists of strings (on one line) and inline tables.
//! `allow` is the effect ceiling of a dependency: its words may perform only
//! these effects (`allow = []` means it must be pure). A dependency without
//! `allow` is not limited.

use crate::error::{Error, Result};

/// One dependency.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DepSpec {
    /// Where the package lives, relative to the directory of the manifest.
    pub path: Option<String>,
    /// A version requirement such as `"^1.2"` (reserved for a package registry).
    pub version: Option<String>,
    /// The effects the dependency may perform; `None` means no limit.
    pub allow: Option<Vec<String>>,
    /// The line of the entry in the manifest.
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub entry: Option<String>,
    /// The dependencies, in the order of the file.
    pub deps: Vec<(String, DepSpec)>,
}

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Str(String),
    List(Vec<String>),
    Table(Vec<(String, Value)>),
}

fn is_key_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

struct ValueParser {
    chars: Vec<char>,
    pos: usize,
    line: usize,
}

impl ValueParser {
    fn new(text: &str, line: usize) -> Self {
        ValueParser {
            chars: text.chars().collect(),
            pos: 0,
            line,
        }
    }

    fn err(&self, msg: &str) -> Error {
        Error::new(msg, self.line, 1)
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
            self.pos += 1;
        }
    }

    fn parse_value(&mut self) -> Result<Value> {
        self.skip_ws();
        match self.peek() {
            Some('"') => Ok(Value::Str(self.parse_string()?)),
            Some('[') => self.parse_list(),
            Some('{') => self.parse_table(),
            Some(_) => Err(self.err("expected a string, a list or an inline table")),
            None => Err(self.err("a value is missing after `=`")),
        }
    }

    /// Parses a string; the next character is the opening quote.
    fn parse_string(&mut self) -> Result<String> {
        self.pos += 1;
        let mut s = String::new();
        loop {
            match self.peek() {
                None => return Err(self.err("unterminated string")),
                Some('"') => {
                    self.pos += 1;
                    return Ok(s);
                }
                Some('\\') => {
                    self.pos += 1;
                    match self.peek() {
                        Some('n') => s.push('\n'),
                        Some('t') => s.push('\t'),
                        Some('"') => s.push('"'),
                        Some('\\') => s.push('\\'),
                        _ => return Err(self.err("unknown escape sequence in a string")),
                    }
                    self.pos += 1;
                }
                Some(c) => {
                    s.push(c);
                    self.pos += 1;
                }
            }
        }
    }

    fn parse_list(&mut self) -> Result<Value> {
        self.pos += 1;
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                Some(']') => {
                    self.pos += 1;
                    return Ok(Value::List(items));
                }
                Some('"') => {
                    items.push(self.parse_string()?);
                    self.skip_ws();
                    match self.peek() {
                        Some(',') => self.pos += 1,
                        Some(']') => {}
                        _ => return Err(self.err("expected `,` or `]` in a list")),
                    }
                }
                _ => return Err(self.err("a list may only contain strings")),
            }
        }
    }

    fn parse_key(&mut self) -> String {
        let start = self.pos;
        while self.pos < self.chars.len() && is_key_char(self.chars[self.pos]) {
            self.pos += 1;
        }
        self.chars[start..self.pos].iter().collect()
    }

    fn parse_table(&mut self) -> Result<Value> {
        self.pos += 1;
        let mut entries: Vec<(String, Value)> = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                Some('}') => {
                    self.pos += 1;
                    return Ok(Value::Table(entries));
                }
                Some(c) if is_key_char(c) => {
                    let key = self.parse_key();
                    self.skip_ws();
                    if self.peek() != Some('=') {
                        return Err(self.err("expected `=` after a key"));
                    }
                    self.pos += 1;
                    let value = self.parse_value()?;
                    if entries.iter().any(|(k, _)| *k == key) {
                        return Err(self.err(&format!("duplicate key `{}`", key)));
                    }
                    entries.push((key, value));
                    self.skip_ws();
                    match self.peek() {
                        Some(',') => self.pos += 1,
                        Some('}') => {}
                        _ => return Err(self.err("expected `,` or `}` in an inline table")),
                    }
                }
                _ => return Err(self.err("expected a key or `}` in an inline table")),
            }
        }
    }

    /// Nothing but white space may follow the value.
    fn finish(&mut self) -> Result<()> {
        self.skip_ws();
        if self.pos < self.chars.len() {
            Err(self.err("unexpected text after the value"))
        } else {
            Ok(())
        }
    }
}

/// Removes a `#` comment, unless the `#` is inside a string.
fn strip_comment(line: &str) -> String {
    let mut out = String::new();
    let mut in_str = false;
    let mut escaped = false;
    for c in line.chars() {
        if in_str {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            }
        } else if c == '"' {
            in_str = true;
            out.push(c);
        } else if c == '#' {
            break;
        } else {
            out.push(c);
        }
    }
    out
}

fn check_dep_name(name: &str, line: usize) -> Result<()> {
    if name.is_empty() || !name.chars().all(is_key_char) {
        return Err(Error::new(
            format!("invalid dependency name `{}`", name),
            line,
            1,
        ));
    }
    if name == "std" {
        return Err(Error::new(
            "`std` is reserved for the standard library and cannot be the name of a dependency",
            line,
            1,
        ));
    }
    Ok(())
}

fn expect_str(key: &str, value: Value, line: usize) -> Result<String> {
    match value {
        Value::Str(s) => Ok(s),
        _ => Err(Error::new(format!("`{}` must be a string", key), line, 1)),
    }
}

fn set_package_key(m: &mut Manifest, key: &str, value: Value, line: usize) -> Result<()> {
    let s = expect_str(key, value, line)?;
    match key {
        "name" => m.name = s,
        "version" => m.version = s,
        "entry" => m.entry = Some(s),
        _ => {
            return Err(Error::new(
                format!("unknown key `{}` in `[package]`", key),
                line,
                1,
            ))
        }
    }
    Ok(())
}

fn set_dep_key(spec: &mut DepSpec, key: &str, value: Value, line: usize) -> Result<()> {
    let duplicate = || Error::new(format!("duplicate key `{}`", key), line, 1);
    match key {
        "path" => {
            if spec.path.is_some() {
                return Err(duplicate());
            }
            spec.path = Some(expect_str(key, value, line)?);
        }
        "version" => {
            if spec.version.is_some() {
                return Err(duplicate());
            }
            spec.version = Some(expect_str(key, value, line)?);
        }
        "allow" => {
            if spec.allow.is_some() {
                return Err(duplicate());
            }
            match value {
                Value::List(items) => spec.allow = Some(items),
                _ => {
                    return Err(Error::new(
                        "`allow` must be a list of effect names, e.g. allow = [\"IO\"]",
                        line,
                        1,
                    ))
                }
            }
        }
        _ => {
            return Err(Error::new(
                format!(
                    "unknown key `{}` for a dependency (expected path, version or allow)",
                    key
                ),
                line,
                1,
            ))
        }
    }
    Ok(())
}

fn dep_from_value(value: Value, line: usize) -> Result<DepSpec> {
    let mut spec = DepSpec {
        line,
        ..DepSpec::default()
    };
    match value {
        Value::Str(req) => spec.version = Some(req),
        Value::Table(entries) => {
            for (k, v) in entries {
                set_dep_key(&mut spec, &k, v, line)?;
            }
        }
        Value::List(_) => {
            return Err(Error::new(
                "a dependency must be a version string or a table such as { path = \"../x\" }",
                line,
                1,
            ))
        }
    }
    Ok(spec)
}

#[derive(Clone, Copy, PartialEq)]
enum Section {
    Outside,
    Package,
    Deps,
    Dep(usize),
}

/// Parses the text of a `stackel.toml`. Positions are lines of that text.
pub fn parse_manifest(text: &str) -> Result<Manifest> {
    let mut m = Manifest::default();
    let mut has_package = false;
    let mut section = Section::Outside;

    for (i, raw) in text.lines().enumerate() {
        let lineno = i + 1;
        let stripped = strip_comment(raw);
        let line = stripped.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix('[') {
            let name = match rest.strip_suffix(']') {
                Some(n) => n.trim(),
                None => return Err(Error::new("a section header must end with `]`", lineno, 1)),
            };
            if name == "package" {
                if has_package {
                    return Err(Error::new("duplicate `[package]` section", lineno, 1));
                }
                has_package = true;
                section = Section::Package;
            } else if name == "deps" {
                section = Section::Deps;
            } else if let Some(dep) = name.strip_prefix("deps.") {
                let dep = dep.trim();
                check_dep_name(dep, lineno)?;
                if m.deps.iter().any(|(n, _)| n == dep) {
                    return Err(Error::new(
                        format!("dependency `{}` is defined more than once", dep),
                        lineno,
                        1,
                    ));
                }
                m.deps.push((
                    dep.to_string(),
                    DepSpec {
                        line: lineno,
                        ..DepSpec::default()
                    },
                ));
                section = Section::Dep(m.deps.len() - 1);
            } else {
                return Err(Error::new(
                    format!("unknown section `[{}]`", name),
                    lineno,
                    1,
                ));
            }
            continue;
        }

        let eq = match line.find('=') {
            Some(p) => p,
            None => return Err(Error::new("expected `key = value`", lineno, 1)),
        };
        let key = line[..eq].trim();
        if key.is_empty() || !key.chars().all(is_key_char) {
            return Err(Error::new(format!("invalid key `{}`", key), lineno, 1));
        }
        let mut parser = ValueParser::new(&line[eq + 1..], lineno);
        let value = parser.parse_value()?;
        parser.finish()?;

        match section {
            Section::Outside => {
                return Err(Error::new(
                    "a key must be inside a section such as `[package]`",
                    lineno,
                    1,
                ))
            }
            Section::Package => set_package_key(&mut m, key, value, lineno)?,
            Section::Deps => {
                check_dep_name(key, lineno)?;
                if m.deps.iter().any(|(n, _)| n == key) {
                    return Err(Error::new(
                        format!("dependency `{}` is defined more than once", key),
                        lineno,
                        1,
                    ));
                }
                let spec = dep_from_value(value, lineno)?;
                m.deps.push((key.to_string(), spec));
            }
            Section::Dep(idx) => set_dep_key(&mut m.deps[idx].1, key, value, lineno)?,
        }
    }

    if !has_package {
        return Err(Error::new("the manifest has no `[package]` section", 1, 1));
    }
    if m.name.is_empty() {
        return Err(Error::new("`[package]` needs a `name`", 1, 1));
    }
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(text: &str) -> String {
        parse_manifest(text).unwrap_err().msg
    }

    #[test]
    fn parses_a_manifest() {
        let m = parse_manifest(
            "# demo\n\
             [package]\n\
             name = \"app\"\n\
             version = \"0.1.0\"\n\
             entry = \"src/app.stk\"\n\
             \n\
             [deps]\n\
             mathx = { path = \"../mathx\", allow = [] }\n\
             json = { path = \"../json\", allow = [\"IO\"] } # trailing comment\n\
             util = \"^1.2\"\n\
             \n\
             [deps.parser]\n\
             path = \"../parser\"\n",
        )
        .unwrap();
        assert_eq!(m.name, "app");
        assert_eq!(m.version, "0.1.0");
        assert_eq!(m.entry.as_deref(), Some("src/app.stk"));
        let names: Vec<&str> = m.deps.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["mathx", "json", "util", "parser"]);
        assert_eq!(m.deps[0].1.path.as_deref(), Some("../mathx"));
        assert_eq!(m.deps[0].1.allow, Some(Vec::new()));
        assert_eq!(m.deps[1].1.allow, Some(vec!["IO".to_string()]));
        assert_eq!(m.deps[2].1.version.as_deref(), Some("^1.2"));
        assert_eq!(m.deps[2].1.path, None);
        assert_eq!(m.deps[3].1.path.as_deref(), Some("../parser"));
        assert_eq!(m.deps[3].1.allow, None);
        assert_eq!(m.deps[3].1.line, 12);
    }

    #[test]
    fn a_hash_inside_a_string_is_not_a_comment() {
        let m = parse_manifest("[package]\nname = \"a#b\"\n").unwrap();
        assert_eq!(m.name, "a#b");
    }

    #[test]
    fn strings_may_contain_escapes() {
        let m = parse_manifest("[package]\nname = \"a\\\"b\"\n").unwrap();
        assert_eq!(m.name, "a\"b");
    }

    #[test]
    fn the_package_section_is_required() {
        assert!(err("").contains("no `[package]`"));
        assert!(err("[package]\nversion = \"1\"\n").contains("needs a `name`"));
        assert!(err("[package]\nname = \"a\"\n[package]\n").contains("duplicate `[package]`"));
    }

    #[test]
    fn malformed_text_is_rejected_with_a_line() {
        let e = parse_manifest("[package]\nname = \"a\"\nbogus line\n").unwrap_err();
        assert_eq!(e.line, 3);
        assert!(e.msg.contains("key = value"));
        assert!(err("name = \"a\"\n").contains("inside a section"));
        assert!(err("[nope]\n").contains("unknown section"));
        assert!(err("[package\n").contains("must end with `]`"));
        assert!(err("[package]\nname = \"a\n").contains("unterminated"));
        assert!(err("[package]\nname = 5\n").contains("expected a string"));
        assert!(err("[package]\nname = \"a\" x\n").contains("after the value"));
        assert!(err("[package]\ncolour = \"red\"\n").contains("unknown key `colour`"));
    }

    #[test]
    fn dependencies_are_checked() {
        let head = "[package]\nname = \"a\"\n[deps]\n";
        assert!(err(&format!("{}std = {{ path = \"x\" }}\n", head)).contains("reserved"));
        assert!(
            err(&format!("{}x = {{ path = \"p\" }}\nx = \"1\"\n", head)).contains("more than once")
        );
        assert!(
            err(&format!("{}x = \"1\"\n[deps.x]\npath = \"p\"\n", head)).contains("more than once")
        );
        assert!(err(&format!("{}x = [\"1\"]\n", head)).contains("version string or a table"));
        assert!(
            err(&format!("{}x = {{ allow = \"IO\" }}\n", head)).contains("list of effect names")
        );
        assert!(err(&format!("{}x = {{ colour = \"red\" }}\n", head)).contains("unknown key"));
        assert!(
            err(&format!("{}x = {{ path = \"a\", path = \"b\" }}\n", head))
                .contains("duplicate key")
        );
    }
}
