//! Parser: tokens -> `Program`.
//!
//! Layout rule: a token at column 0 starts a new top-level item; every
//! following token that is indented belongs to the same item.
//!
//! ```text
//! item  := [pub] name "::" type-tokens
//!        | [pub] name "=" body
//!        | [pub] "type" Name {param} "=" ["|"] Ctor {type} {"|" Ctor {type}}
//!        | "module" name
//!        | "use" path ["from" string] ["(" name {"," name} ")"]
//! body  := node* [ guard | match ]
//! guard := "|" "then" node* "|" "else" body
//! match := "match" { "|" Ctor body }
//! node  := int | float | string | word | "[" body "]"
//! ```
//!
//! A guard or a `match` always ends its body. A `|` followed by `then` starts
//! a guard; a `|` followed by a constructor (uppercase) starts a `match` arm.

use std::rc::Rc;

use crate::ast::{Arm, CtorDef, Def, Node, Program, Sig, TypeDef, Use};
use crate::error::{Error, Result};
use crate::lexer::{Tok, Token};

pub fn parse(tokens: &[Token]) -> Result<Program> {
    let mut program = Program::default();
    if tokens.is_empty() {
        return Ok(program);
    }
    if tokens[0].col != 0 {
        return Err(Error::new(
            "top-level items must start at column 1",
            tokens[0].line,
            tokens[0].col + 1,
        ));
    }
    let mut start = 0;
    for i in 1..=tokens.len() {
        if i == tokens.len() || tokens[i].col == 0 {
            parse_item(&tokens[start..i], &mut program)?;
            start = i;
        }
    }
    Ok(program)
}

struct Cursor<'a> {
    toks: &'a [Token],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(toks: &'a [Token]) -> Self {
        Cursor { toks, pos: 0 }
    }

    fn peek(&self) -> Option<&'a Token> {
        self.toks.get(self.pos)
    }

    fn peek2(&self) -> Option<&'a Token> {
        self.toks.get(self.pos + 1)
    }

    fn bump(&mut self) -> Option<&'a Token> {
        let t = self.toks.get(self.pos);
        if t.is_some() {
            self.pos += 1;
        }
        t
    }
}

fn is_upper(s: &str) -> bool {
    s.chars().next().map_or(false, |c| c.is_uppercase())
}

/// A constructor, possibly qualified by a module (`Some`, `opt.Some`).
fn is_ctor_ref(s: &str) -> bool {
    is_upper(s.rsplit('.').next().unwrap_or(s))
}

fn is_lower(s: &str) -> bool {
    s.chars().next().map_or(false, |c| c.is_lowercase())
}

fn err_at(t: &Token, msg: &str) -> Error {
    Error::new(msg, t.line, t.col + 1)
}

fn expect_word(cur: &mut Cursor, after: &Token, what: &str) -> Result<String> {
    match cur.bump() {
        Some(Token {
            tok: Tok::Word(w), ..
        }) => Ok(w.clone()),
        Some(t) => Err(err_at(t, &format!("expected {}", what))),
        None => Err(err_at(after, &format!("expected {}", what))),
    }
}

fn expect_keyword(cur: &mut Cursor, after: &Token, kw: &str) -> Result<()> {
    match cur.bump() {
        Some(Token {
            tok: Tok::Word(w), ..
        }) if w == kw => Ok(()),
        Some(t) => Err(err_at(t, &format!("expected `{}`", kw))),
        None => Err(err_at(after, &format!("expected `{}`", kw))),
    }
}

fn expect_end(cur: &mut Cursor) -> Result<()> {
    match cur.peek() {
        Some(t) => Err(err_at(t, "unexpected token")),
        None => Ok(()),
    }
}

fn parse_item(chunk: &[Token], program: &mut Program) -> Result<()> {
    let mut cur = Cursor::new(chunk);
    let first = cur.bump().expect("a chunk is never empty");
    let mut public = false;
    let mut head = first;
    if let Tok::Word(w) = &first.tok {
        if w == "pub" {
            public = true;
            head = match cur.bump() {
                Some(t) => t,
                None => return Err(err_at(first, "expected a definition after `pub`")),
            };
        }
    }
    let name = match &head.tok {
        Tok::Word(w) => w.clone(),
        _ => {
            return Err(err_at(
                head,
                "expected a word at the start of a top-level item",
            ))
        }
    };
    match name.as_str() {
        "module" if !public => parse_module(&mut cur, head, program),
        "use" if !public => parse_use(&mut cur, head, program),
        "type" => parse_type(&mut cur, head, public, program),
        _ => parse_def_or_sig(&mut cur, head, name, public, program),
    }
}

fn parse_module(cur: &mut Cursor, head: &Token, program: &mut Program) -> Result<()> {
    if program.module.is_some() {
        return Err(err_at(head, "duplicate `module` declaration"));
    }
    let name = expect_word(cur, head, "a module name")?;
    expect_end(cur)?;
    program.module = Some(name);
    Ok(())
}

fn parse_use(cur: &mut Cursor, head: &Token, program: &mut Program) -> Result<()> {
    let path = expect_word(cur, head, "a module path after `use`")?;

    let mut source = None;
    if let Some(Token {
        tok: Tok::Word(w), ..
    }) = cur.peek()
    {
        if w == "from" {
            cur.bump();
            match cur.bump() {
                Some(Token {
                    tok: Tok::Str(s), ..
                }) => source = Some(s.clone()),
                Some(t) => return Err(err_at(t, "expected a string after `from`")),
                None => return Err(err_at(head, "expected a string after `from`")),
            }
        }
    }

    let mut names = Vec::new();
    if let Some(Token {
        tok: Tok::LParen, ..
    }) = cur.peek()
    {
        cur.bump();
        loop {
            match cur.bump() {
                Some(Token {
                    tok: Tok::Word(w), ..
                }) => names.push(w.clone()),
                Some(t) => return Err(err_at(t, "expected a name in the import list")),
                None => return Err(err_at(head, "unterminated import list")),
            }
            match cur.bump() {
                Some(Token {
                    tok: Tok::Comma, ..
                }) => continue,
                Some(Token {
                    tok: Tok::RParen, ..
                }) => break,
                Some(t) => return Err(err_at(t, "expected `,` or `)`")),
                None => return Err(err_at(head, "unterminated import list")),
            }
        }
    }

    expect_end(cur)?;
    program.uses.push(Use {
        path,
        source,
        names,
        line: head.line,
    });
    Ok(())
}

fn parse_type(cur: &mut Cursor, head: &Token, public: bool, program: &mut Program) -> Result<()> {
    let name = expect_word(cur, head, "a type name after `type`")?;
    if !is_upper(&name) {
        return Err(err_at(
            head,
            "type names must start with an uppercase letter",
        ));
    }
    let mut params = Vec::new();
    loop {
        match cur.bump() {
            Some(Token {
                tok: Tok::Equals, ..
            }) => break,
            Some(Token {
                tok: Tok::Word(w), ..
            }) if is_lower(w) => params.push(w.clone()),
            Some(t) => {
                return Err(err_at(
                    t,
                    "expected a lowercase type parameter or `=` in the type declaration",
                ))
            }
            None => return Err(err_at(head, "expected `=` in the type declaration")),
        }
    }
    // A leading `|` is allowed, so that constructors can be listed one per line.
    if let Some(Token { tok: Tok::Bar, .. }) = cur.peek() {
        cur.bump();
    }
    let mut ctors = Vec::new();
    loop {
        let (ctor, line) = match cur.bump() {
            Some(Token {
                tok: Tok::Word(w),
                line,
                ..
            }) if is_upper(w) => (w.clone(), *line),
            Some(t) => {
                return Err(err_at(
                    t,
                    "expected a constructor name (it must start with an uppercase letter)",
                ))
            }
            None => return Err(err_at(head, "expected a constructor name")),
        };
        let mut fields = Vec::new();
        while let Some(t) = cur.peek() {
            if t.tok == Tok::Bar {
                break;
            }
            fields.push(t.tok.clone());
            cur.bump();
        }
        ctors.push(CtorDef {
            name: ctor,
            fields,
            line,
        });
        if cur.peek().is_some() {
            cur.bump(); // the `|`
        } else {
            break;
        }
    }
    program.types.push(TypeDef {
        name,
        public,
        params,
        ctors,
        line: head.line,
    });
    Ok(())
}

fn parse_def_or_sig(
    cur: &mut Cursor,
    head: &Token,
    name: String,
    public: bool,
    program: &mut Program,
) -> Result<()> {
    if name == "match" {
        return Err(err_at(head, "`match` is a keyword and cannot be defined"));
    }
    match cur.bump() {
        Some(Token {
            tok: Tok::ColonColon,
            ..
        }) => {
            let mut tokens = Vec::new();
            while let Some(t) = cur.bump() {
                tokens.push(t.tok.clone());
            }
            if tokens.is_empty() {
                return Err(err_at(head, "empty type signature"));
            }
            program.sigs.push(Sig {
                name,
                public,
                tokens,
                line: head.line,
            });
            Ok(())
        }
        Some(Token {
            tok: Tok::Equals, ..
        }) => {
            let body = parse_body(cur, true)?;
            if let Some(t) = cur.peek() {
                return Err(err_at(t, "unexpected token in definition body"));
            }
            program.defs.push(Def {
                name,
                public,
                body: Rc::new(body),
                line: head.line,
            });
            Ok(())
        }
        Some(t) => Err(err_at(t, "expected `::` or `=` after the name")),
        None => Err(err_at(head, "expected `::` or `=` after the name")),
    }
}

/// Parses nodes until the end of the chunk, a `]`, or a `|`.
/// If `allow_guard` is set and a `|` follows, the guard is parsed and appended as the last node.
fn parse_body(cur: &mut Cursor, allow_guard: bool) -> Result<Vec<Node>> {
    let mut nodes = Vec::new();
    loop {
        let t = match cur.peek() {
            Some(t) => t,
            None => break,
        };
        match &t.tok {
            Tok::RBracket | Tok::Bar => break,
            Tok::Int(n) => {
                nodes.push(Node::Int(*n));
                cur.bump();
            }
            Tok::Float(x) => {
                nodes.push(Node::Float(*x));
                cur.bump();
            }
            Tok::Str(s) => {
                nodes.push(Node::Str(s.clone()));
                cur.bump();
            }
            Tok::Word(w) if w == "match" => {
                let m = parse_match(cur)?;
                nodes.push(m);
                // A `match` ends the body.
                return Ok(nodes);
            }
            Tok::Word(w) if w == "let" => {
                let node = parse_let(cur, allow_guard)?;
                nodes.push(node);
                // A `let` ends the body: its own body runs to the end.
                return Ok(nodes);
            }
            Tok::Word(w) => {
                nodes.push(Node::Word {
                    name: w.clone(),
                    line: t.line,
                    col: t.col + 1,
                });
                cur.bump();
            }
            Tok::LBracket => {
                cur.bump();
                let inner = parse_body(cur, true)?;
                match cur.bump() {
                    Some(Token {
                        tok: Tok::RBracket, ..
                    }) => {}
                    Some(u) => return Err(err_at(u, "expected `]`")),
                    None => return Err(err_at(t, "unclosed `[`")),
                }
                nodes.push(Node::Quote(Rc::new(inner)));
            }
            _ => return Err(err_at(t, "unexpected token in body")),
        }
    }
    if allow_guard {
        if let (
            Some(Token { tok: Tok::Bar, .. }),
            Some(Token {
                tok: Tok::Word(w), ..
            }),
        ) = (cur.peek(), cur.peek2())
        {
            if w == "then" {
                let guard = parse_guard(cur)?;
                nodes.push(guard);
            }
        }
    }
    Ok(nodes)
}

/// Parses `let a b c in body`; the next token is the `let` word. The body runs to the end of
/// the enclosing body.
fn parse_let(cur: &mut Cursor, allow_guard: bool) -> Result<Node> {
    let kw = cur.bump().expect("the caller checked for `let`");
    let mut names: Vec<String> = Vec::new();
    loop {
        let t = match cur.bump() {
            Some(t) => t,
            None => return Err(err_at(kw, "`let` needs `in`")),
        };
        match &t.tok {
            Tok::Word(n) if n == "in" => break,
            Tok::Word(n) => {
                if !is_lower(n) || n.contains('.') || n == "match" || n == "let" {
                    return Err(err_at(
                        t,
                        "a local name must start with a lowercase letter and cannot contain `.`",
                    ));
                }
                if names.contains(n) {
                    return Err(err_at(
                        t,
                        &format!("the name `{}` is bound twice by this `let`", n),
                    ));
                }
                names.push(n.clone());
            }
            _ => return Err(err_at(t, "expected a name or `in` after `let`")),
        }
    }
    if names.is_empty() {
        return Err(err_at(kw, "`let` needs at least one name before `in`"));
    }
    let body = parse_body(cur, allow_guard)?;
    Ok(Node::Let {
        names,
        body: Rc::new(body),
        line: kw.line,
        col: kw.col + 1,
    })
}

/// Parses `match | Ctor body | Ctor body ...`; the next token is the `match` word.
fn parse_match(cur: &mut Cursor) -> Result<Node> {
    let kw = cur.bump().expect("the caller checked for `match`");
    let mut arms = Vec::new();
    loop {
        match (cur.peek(), cur.peek2()) {
            (
                Some(Token { tok: Tok::Bar, .. }),
                Some(Token {
                    tok: Tok::Word(w), ..
                }),
            ) if is_ctor_ref(w) => {}
            _ => break,
        }
        let bar = cur.bump().expect("peeked");
        let name_tok = cur.bump().expect("peeked");
        let ctor = match &name_tok.tok {
            Tok::Word(w) => w.clone(),
            _ => unreachable!("peeked a word"),
        };
        let body = parse_body(cur, true)?;
        arms.push(Arm {
            ctor,
            body: Rc::new(body),
            line: bar.line,
            col: bar.col + 1,
        });
    }
    if arms.is_empty() {
        return Err(err_at(
            kw,
            "`match` needs at least one `| Constructor ...` arm",
        ));
    }
    Ok(Node::Match {
        arms,
        line: kw.line,
        col: kw.col + 1,
    })
}

fn parse_guard(cur: &mut Cursor) -> Result<Node> {
    let bar = cur.bump().expect("the caller checked for `|`");
    expect_keyword(cur, bar, "then")?;
    let then_body = parse_body(cur, false)?;
    let bar2 = match cur.bump() {
        Some(t) if t.tok == Tok::Bar => t,
        Some(t) => return Err(err_at(t, "expected `| else` after the `then` branch")),
        None => return Err(err_at(bar, "missing `| else` branch (`else` is required)")),
    };
    expect_keyword(cur, bar2, "else")?;
    let else_body = parse_body(cur, true)?;
    Ok(Node::If {
        then_branch: Rc::new(then_body),
        else_branch: Rc::new(else_body),
        line: bar.line,
        col: bar.col + 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    fn parse_src(s: &str) -> Result<Program> {
        parse(&lex(s)?)
    }

    fn word_name(n: &Node) -> &str {
        match n {
            Node::Word { name, .. } => name,
            other => panic!("expected a word, found {:?}", other),
        }
    }

    #[test]
    fn simple_definition() {
        let p = parse_src("main = 1 2 add").unwrap();
        assert_eq!(p.defs.len(), 1);
        let body = &p.defs[0].body;
        assert_eq!(body.len(), 3);
        assert_eq!(body[0], Node::Int(1));
        assert_eq!(word_name(&body[2]), "add");
    }

    #[test]
    fn signature_and_definition() {
        let p = parse_src("f :: Int -- Int\nf = 1 add").unwrap();
        assert_eq!(p.sigs.len(), 1);
        assert_eq!(p.sigs[0].name, "f");
        assert_eq!(p.sigs[0].tokens.len(), 3);
        assert_eq!(p.defs.len(), 1);
    }

    #[test]
    fn guard_with_indented_continuation() {
        let p = parse_src("f = dup 0 lt\n  | then neg\n  | else").unwrap();
        let body = &p.defs[0].body;
        assert_eq!(body.len(), 4);
        match &body[3] {
            Node::If {
                then_branch,
                else_branch,
                ..
            } => {
                assert_eq!(then_branch.len(), 1);
                assert_eq!(else_branch.len(), 0);
            }
            other => panic!("expected a guard, found {:?}", other),
        }
    }

    #[test]
    fn nested_guard_in_else() {
        let src = "f = dup 0 lt\n  | then 1\n  | else 0 eq\n    | then 2\n    | else 3";
        let p = parse_src(src).unwrap();
        match p.defs[0].body.last().unwrap() {
            Node::If { else_branch, .. } => match else_branch.last().unwrap() {
                Node::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    assert_eq!(then_branch.as_ref(), &vec![Node::Int(2)]);
                    assert_eq!(else_branch.as_ref(), &vec![Node::Int(3)]);
                }
                other => panic!("expected a nested guard, found {:?}", other),
            },
            other => panic!("expected a guard, found {:?}", other),
        }
    }

    #[test]
    fn quotation() {
        let p = parse_src("f = [ 1 add ] apply").unwrap();
        match &p.defs[0].body[0] {
            Node::Quote(q) => assert_eq!(q.len(), 2),
            other => panic!("expected a quotation, found {:?}", other),
        }
    }

    #[test]
    fn module_and_use() {
        let p =
            parse_src("module m\nuse std.io (print, show)\nuse json from \"pkg:json@^1\"").unwrap();
        assert_eq!(p.module.as_deref(), Some("m"));
        assert_eq!(p.uses.len(), 2);
        assert_eq!(p.uses[0].path, "std.io");
        assert_eq!(
            p.uses[0].names,
            vec!["print".to_string(), "show".to_string()]
        );
        assert_eq!(p.uses[1].source.as_deref(), Some("pkg:json@^1"));
    }

    #[test]
    fn pub_marks_public() {
        let p = parse_src("pub f = 1").unwrap();
        assert!(p.defs[0].public);
    }

    #[test]
    fn errors() {
        assert!(parse_src("f = dup | then drop").is_err()); // `else` is required
        assert!(parse_src("f = [ 1").is_err()); // unclosed quotation
        assert!(parse_src("f = 1 ]").is_err()); // stray `]`
        assert!(parse_src("  f = 1").is_err()); // must start at column 1
        assert!(parse_src("f 1").is_err()); // missing `=`
        assert!(parse_src("type T = ").is_err()); // a data type needs at least one constructor
    }

    #[test]
    fn type_declarations() {
        let src = "type Option a = None | Some a\ntype List a = Nil | Cons a (List a)\ntype Color = Red | Green | Blue";
        let p = parse_src(src).unwrap();
        assert_eq!(p.types.len(), 3);

        let option = &p.types[0];
        assert_eq!(option.name, "Option");
        assert_eq!(option.params, vec!["a".to_string()]);
        assert_eq!(option.ctors.len(), 2);
        assert_eq!(option.ctors[0].name, "None");
        assert!(option.ctors[0].fields.is_empty());
        assert_eq!(option.ctors[1].fields, vec![Tok::Word("a".to_string())]);

        let list = &p.types[1];
        assert_eq!(list.ctors[1].name, "Cons");
        assert_eq!(list.ctors[1].fields.len(), 5);

        let color = &p.types[2];
        assert!(color.params.is_empty());
        assert_eq!(color.ctors.len(), 3);
    }

    #[test]
    fn type_declaration_over_several_lines() {
        let p = parse_src("pub type Shape =\n  | Circle Int\n  | Square Int\n").unwrap();
        assert!(p.types[0].public);
        assert_eq!(p.types[0].ctors.len(), 2);
        assert_eq!(p.types[0].ctors[0].name, "Circle");
    }

    #[test]
    fn match_arms() {
        let p = parse_src("f = match\n  | None 0\n  | Some 1 add").unwrap();
        let body = &p.defs[0].body;
        assert_eq!(body.len(), 1);
        match &body[0] {
            Node::Match { arms, .. } => {
                assert_eq!(arms.len(), 2);
                assert_eq!(arms[0].ctor, "None");
                assert_eq!(arms[0].body.len(), 1);
                assert_eq!(arms[1].ctor, "Some");
                assert_eq!(arms[1].body.len(), 2);
            }
            other => panic!("expected a match, found {:?}", other),
        }
    }

    #[test]
    fn a_guard_can_end_a_match_arm() {
        let src = "f = match\n  | None 0\n  | Some dup 0 lt\n    | then neg\n    | else";
        let p = parse_src(src).unwrap();
        match &p.defs[0].body[0] {
            Node::Match { arms, .. } => {
                assert_eq!(arms.len(), 2);
                assert_eq!(arms[1].body.len(), 4);
                assert!(matches!(arms[1].body[3], Node::If { .. }));
            }
            other => panic!("expected a match, found {:?}", other),
        }
    }

    #[test]
    fn match_inside_a_quotation() {
        let p = parse_src("g = [ match | A 1 | B 2 ] apply").unwrap();
        match &p.defs[0].body[0] {
            Node::Quote(q) => match &q[0] {
                Node::Match { arms, .. } => assert_eq!(arms.len(), 2),
                other => panic!("expected a match, found {:?}", other),
            },
            other => panic!("expected a quotation, found {:?}", other),
        }
    }

    #[test]
    fn data_declaration_and_match_errors() {
        assert!(parse_src("f = match").is_err()); // no arms
        assert!(parse_src("match = 1").is_err()); // keyword
        assert!(parse_src("type T").is_err()); // no `=`
        assert!(parse_src("type t = A").is_err()); // type names are uppercase
        assert!(parse_src("type T = a").is_err()); // constructor names are uppercase
        assert!(parse_src("type T =").is_err()); // no constructors
    }

    #[test]
    fn match_arms_may_name_qualified_constructors() {
        let p = parse_src("f = match\n  | opt.None 0\n  | opt.Some").unwrap();
        match &p.defs[0].body[0] {
            Node::Match { arms, .. } => {
                assert_eq!(arms.len(), 2);
                assert_eq!(arms[0].ctor, "opt.None");
                assert_eq!(arms[1].ctor, "opt.Some");
            }
            other => panic!("expected a match, found {:?}", other),
        }
    }

    #[test]
    fn let_binds_names_for_the_rest_of_the_body() {
        let p = parse_src("f = 1 2 let a b in b a sub").unwrap();
        let body = &p.defs[0].body;
        assert_eq!(body.len(), 3);
        match &body[2] {
            Node::Let { names, body, .. } => {
                assert_eq!(names, &vec!["a".to_string(), "b".to_string()]);
                assert_eq!(body.len(), 3);
            }
            other => panic!("expected a let, found {:?}", other),
        }
    }

    #[test]
    fn a_let_body_may_end_with_a_guard_or_a_match() {
        let p = parse_src("f = let a in a 0 lt\n  | then a neg\n  | else a").unwrap();
        match &p.defs[0].body[0] {
            Node::Let { body, .. } => assert!(matches!(body.last(), Some(Node::If { .. }))),
            other => panic!("expected a let, found {:?}", other),
        }
        let p = parse_src("f = let a in a match\n  | None 0\n  | Some").unwrap();
        match &p.defs[0].body[0] {
            Node::Let { body, .. } => assert!(matches!(body.last(), Some(Node::Match { .. }))),
            other => panic!("expected a let, found {:?}", other),
        }
    }

    #[test]
    fn let_errors() {
        assert!(parse_src("f = let a b").is_err());
        assert!(parse_src("f = let in 1").is_err());
        assert!(parse_src("f = let a a in 1").is_err());
        assert!(parse_src("f = let A in 1").is_err());
        assert!(parse_src("f = let a.b in 1").is_err());
        assert!(parse_src("f = let 1 in 1").is_err());
    }
}
