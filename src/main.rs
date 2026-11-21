use std::env;
use std::io::{self, Write};
use std::path::Path;
use std::process;

const USAGE: &str = "usage: stackel run [<file.stk> | <package directory> [arguments...]]
       stackel check [<file.stk> | <package directory>]
       stackel test [<file.stk> | <package directory>]
       stackel repl
       stackel --version";

fn usage() -> ! {
    eprintln!("{}", USAGE);
    process::exit(2);
}

/// Prints an error and exits. A program that called `exit` ends with its status instead.
fn fail(path: &str, e: stackel::error::Error) -> ! {
    if let Some(code) = e.exit {
        process::exit(code);
    }
    if e.file.is_some() {
        eprintln!("{}", e);
    } else {
        eprintln!("{}: {}", path, e);
    }
    if let Some(snippet) = &e.snippet {
        eprintln!("{}", snippet);
    }
    process::exit(1);
}

fn main() {
    // Deeply recursive Stackel programs need a deep native stack.
    let worker = std::thread::Builder::new()
        .stack_size(1 << 28)
        .spawn(run_cli);
    match worker {
        Ok(handle) => {
            if handle.join().is_err() {
                process::exit(101);
            }
        }
        Err(e) => {
            eprintln!("cannot start the interpreter thread: {}", e);
            process::exit(1);
        }
    }
}

fn run_cli() {
    let args: Vec<String> = env::args().collect();
    let command = match args.get(1) {
        Some(c) => c.as_str(),
        None => usage(),
    };
    if command == "--version" || command == "-V" {
        println!("stackel {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if command == "--help" || command == "-h" {
        println!("{}", USAGE);
        return;
    }
    if command == "repl" {
        if args.len() > 2 {
            usage();
        }
        let stdin = io::stdin();
        let mut input = stdin.lock();
        let mut out = io::stdout();
        stackel::repl::run(&mut input, &mut out);
        return;
    }
    if command != "run" && command != "check" && command != "test" {
        usage();
    }
    let path = args.get(2).map(|p| p.as_str()).unwrap_or(".");
    let program_args: Vec<String> = if args.len() > 3 {
        args[3..].to_vec()
    } else {
        Vec::new()
    };
    if command != "run" && !program_args.is_empty() {
        usage();
    }

    let loaded = match stackel::load_path(Path::new(path)) {
        Ok(l) => l,
        Err(e) => fail(path, e),
    };

    if command == "check" {
        match loaded.check() {
            Ok(()) => println!("{}: ok", path),
            Err(e) => fail(path, e),
        }
        return;
    }

    if command == "test" {
        let mut out = io::stdout();
        match loaded.test(&mut out) {
            Ok((_, failed)) => {
                if failed > 0 {
                    process::exit(1);
                }
            }
            Err(e) => fail(path, e),
        }
        return;
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let result = loaded.run_with_args(&mut out, program_args);
    let _ = out.flush();
    if let Err(e) = result {
        fail(path, e);
    }
}
