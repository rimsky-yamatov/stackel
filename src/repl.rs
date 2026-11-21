//! An interactive session: `stackel repl`.
//!
//! A line that defines something (a word, a signature, a type, an import) is added to the
//! definitions of the session, if the whole session still checks. Any other line is a sequence
//! of words: it runs on the stack of the session, and the stack is shown afterwards.

use std::io::{BufRead, Write};

use crate::check;
use crate::error::Error;
use crate::eval::Interp;
use crate::lexer::{self, Tok};
use crate::modules;
use crate::value::Value;

const HELP: &str = "\
Type words to run them on the stack, or a definition (`double = dup add`) to keep it.
  :type <words>   show the type of the words
  :stack          show the stack          :clear   empty the stack
  :items          show the definitions    :reset   forget everything
  :load <file>    add the definitions of a file
  :{ ... :}       one input over several lines
  :quit           leave";

pub struct Session {
    /// The accepted definitions, as source text.
    items: Vec<String>,
    /// The stack that the next line starts with.
    stack: Vec<Value>,
    /// Set when the session is over (`:quit`, `exit`).
    done: bool,
}

impl Default for Session {
    fn default() -> Self {
        Session::new()
    }
}

fn say(out: &mut dyn Write, text: &str) {
    let _ = writeln!(out, "{}", text);
}

/// The tokens of `text` if it starts a definition (of a word, a signature, a type, an import).
/// A leading `pub` is dropped.
fn item_tokens(text: &str) -> Option<Vec<Tok>> {
    let tokens = lexer::lex(text).ok()?;
    let mut toks: Vec<Tok> = tokens.into_iter().map(|t| t.tok).collect();
    if matches!(toks.first(), Some(Tok::Word(w)) if w == "pub") {
        toks.remove(0);
    }
    let is_item = match (toks.first(), toks.get(1)) {
        (Some(Tok::Word(w)), _) if w == "type" || w == "use" || w == "module" => true,
        (Some(Tok::Word(_)), Some(Tok::Equals)) => true,
        (Some(Tok::Word(_)), Some(Tok::ColonColon)) => true,
        _ => false,
    };
    if is_item {
        Some(toks)
    } else {
        None
    }
}

/// What a definition defines, so that a new definition can replace an older one.
fn item_key(text: &str) -> Option<String> {
    let toks = item_tokens(text)?;
    match (toks.first(), toks.get(1)) {
        (Some(Tok::Word(kw)), Some(Tok::Word(name))) if kw == "type" => {
            Some(format!("type {}", name))
        }
        (Some(Tok::Word(name)), Some(Tok::ColonColon)) => Some(format!("sig {}", name)),
        (Some(Tok::Word(name)), Some(Tok::Equals)) => Some(format!("def {}", name)),
        _ => None,
    }
}

/// Continuation lines of a body must be indented to belong to its item.
fn indent_rest(text: &str) -> String {
    text.replace('\n', "\n  ")
}

fn load_and_check(source: &str) -> Result<modules::Loaded, String> {
    let loaded = modules::load_source(source).map_err(|e| e.msg)?;
    loaded.check().map_err(|e| e.msg)?;
    Ok(loaded)
}

fn error_text(e: &Error) -> String {
    if e.failure {
        format!("uncaught failure: {}", e.msg)
    } else {
        e.msg.clone()
    }
}

/// A value as the session shows it: constructors without their module, and strings quoted.
fn show_value(v: &Value, nested: bool) -> String {
    match v {
        Value::Int(n) => n.to_string(),
        Value::Float(x) => format!("{:?}", x),
        Value::Bool(b) => b.to_string(),
        Value::Str(s) => format!("{:?}", s),
        Value::Quote(_) => "[...]".to_string(),
        Value::Data(d) => {
            let name = d.ctor.rsplit('.').next().unwrap_or(&d.ctor).to_string();
            if d.fields.is_empty() {
                name
            } else {
                let fields: Vec<String> = d.fields.iter().map(|f| show_value(f, true)).collect();
                let text = format!("{} {}", name, fields.join(" "));
                if nested {
                    format!("({})", text)
                } else {
                    text
                }
            }
        }
    }
}

impl Session {
    pub fn new() -> Self {
        Session {
            items: Vec::new(),
            stack: Vec::new(),
            done: false,
        }
    }

    pub fn is_done(&self) -> bool {
        self.done
    }

    /// Whether `text` starts a definition. Such an input may go on in the lines that follow.
    pub fn starts_item(text: &str) -> bool {
        item_tokens(text).is_some()
    }

    fn stack_text(&self) -> String {
        if self.stack.is_empty() {
            "stack: empty".to_string()
        } else {
            let shown: Vec<String> = self.stack.iter().map(|v| show_value(v, false)).collect();
            format!("stack: {}", shown.join(" "))
        }
    }

    /// Handles one input: a command, a definition, or words to run. Everything the session
    /// has to say, and the output of the program, goes to `out`.
    pub fn enter(&mut self, input: &str, out: &mut dyn Write) {
        let text = input.trim();
        if text.is_empty() {
            return;
        }
        if let Some(command) = text.strip_prefix(':') {
            self.command(command.trim(), out);
        } else if Session::starts_item(text) {
            self.define(text, out);
        } else {
            self.evaluate(text, out);
        }
    }

    fn command(&mut self, text: &str, out: &mut dyn Write) {
        let (name, rest) = match text.split_once(char::is_whitespace) {
            Some((n, r)) => (n, r.trim()),
            None => (text, ""),
        };
        match name {
            "quit" | "q" => self.done = true,
            "help" | "h" | "?" => say(out, HELP),
            "stack" | "s" => say(out, &self.stack_text()),
            "clear" => {
                self.stack.clear();
                say(out, &self.stack_text());
            }
            "reset" => {
                self.items.clear();
                self.stack.clear();
                say(out, "ok");
            }
            "items" => {
                if self.items.is_empty() {
                    say(out, "(no definitions)");
                }
                for item in &self.items {
                    say(out, item);
                }
            }
            "type" | "t" => self.show_type(rest, out),
            "load" | "l" => self.load_file(rest, out),
            _ => say(out, &format!("unknown command `:{}` (try :help)", name)),
        }
    }

    fn show_type(&self, words: &str, out: &mut dyn Write) {
        if words.is_empty() {
            say(out, "usage: :type <words>");
            return;
        }
        let source = format!(
            "{}\nrepl_type = {}\n",
            self.items.join("\n"),
            indent_rest(words)
        );
        let result = modules::load_source(&source)
            .map_err(|e| e.msg)
            .and_then(|l| check::type_of(&l.program, "repl_type").map_err(|e| e.msg));
        match result {
            Ok(t) => say(out, &format!("{} :: {}", words, t)),
            Err(msg) => say(out, &format!("error: {}", msg)),
        }
    }

    fn load_file(&mut self, path: &str, out: &mut dyn Write) {
        if path.is_empty() {
            say(out, "usage: :load <file>");
            return;
        }
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                say(out, &format!("error: cannot read {}: {}", path, e));
                return;
            }
        };
        let mut items = self.items.clone();
        items.push(text);
        match load_and_check(&items.join("\n")) {
            Ok(_) => {
                self.items = items;
                say(out, "ok");
            }
            Err(msg) => say(out, &format!("error: {}", msg)),
        }
    }

    /// Adds a definition, if the session still checks with it. It replaces an earlier
    /// definition of the same word, signature or type.
    fn define(&mut self, text: &str, out: &mut dyn Write) {
        let key = item_key(text);
        let mut items: Vec<String> = self
            .items
            .iter()
            .filter(|old| key.is_none() || item_key(old) != key)
            .cloned()
            .collect();
        items.push(text.to_string());
        match load_and_check(&items.join("\n")) {
            Ok(_) => {
                self.items = items;
                say(out, "ok");
            }
            Err(msg) => say(out, &format!("error: {}", msg)),
        }
    }

    /// Runs the words on the stack of the session.
    fn evaluate(&mut self, text: &str, out: &mut dyn Write) {
        let source = format!(
            "{}\nrepl_it = {}\n",
            self.items.join("\n"),
            indent_rest(text)
        );
        let loaded = match load_and_check(&source) {
            Ok(l) => l,
            Err(msg) => {
                say(out, &format!("error: {}", msg));
                return;
            }
        };
        let before = self.stack.clone();
        let (result, after) = {
            let mut interp = Interp::new(&mut *out);
            interp.stack = before.clone();
            let result = match interp.load(&loaded.program) {
                Ok(()) => interp.run_word("repl_it"),
                Err(e) => Err(e),
            };
            (result, std::mem::take(&mut interp.stack))
        };
        match result {
            Ok(()) => {
                self.stack = after;
                say(out, &self.stack_text());
            }
            Err(e) => match e.exit {
                Some(code) => {
                    say(out, &format!("exit {}", code));
                    self.done = true;
                }
                None => {
                    self.stack = before;
                    say(out, &format!("error: {}", error_text(&e)));
                }
            },
        }
    }
}

fn flush_pending(session: &mut Session, pending: &mut Vec<String>, out: &mut dyn Write) {
    if pending.is_empty() {
        return;
    }
    let text = pending.join("\n");
    pending.clear();
    session.enter(&text, out);
}

/// Reads lines from `input` and runs them in a session; everything goes to `out`.
///
/// A definition goes on in the following lines that are indented; an empty line ends it.
/// `:{` and `:}` enclose one input of several lines.
pub fn run(input: &mut dyn BufRead, out: &mut dyn Write) {
    let mut session = Session::new();
    say(
        out,
        "Stackel: type words to run them, :help for help, :quit to leave",
    );
    let mut pending: Vec<String> = Vec::new();
    let mut block: Option<Vec<String>> = None;
    loop {
        let prompt = if block.is_some() || !pending.is_empty() {
            "| "
        } else {
            "> "
        };
        let _ = write!(out, "{}", prompt);
        let _ = out.flush();

        let mut line = String::new();
        match input.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let line = line.trim_end_matches(|c| c == '\n' || c == '\r').to_string();

        if block.is_some() {
            if line.trim() == ":}" {
                let text = block.take().unwrap_or_default().join("\n");
                session.enter(&text, out);
            } else if let Some(lines) = block.as_mut() {
                lines.push(line);
            }
        } else if line.trim() == ":{" {
            flush_pending(&mut session, &mut pending, out);
            block = Some(Vec::new());
        } else if !pending.is_empty()
            && !line.trim().is_empty()
            && line.starts_with(char::is_whitespace)
        {
            pending.push(line);
        } else {
            flush_pending(&mut session, &mut pending, out);
            if line.trim().is_empty() {
                // an empty line only ends a definition
            } else if Session::starts_item(line.trim()) {
                pending.push(line);
            } else {
                session.enter(&line, out);
            }
        }
        if session.is_done() {
            break;
        }
    }
    if !session.is_done() {
        flush_pending(&mut session, &mut pending, out);
    }
}
