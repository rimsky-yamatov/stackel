use std::fmt;

/// A compile-time or run-time error.
///
/// `line == 0` means the error has no source position (typically a run-time error).
#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    pub msg: String,
    pub line: usize,
    /// 1-based column.
    pub col: usize,
    /// The source file, for programs that consist of several files.
    pub file: Option<String>,
    /// Set when the program asked to stop with an exit status (`exit`). That is not a failure.
    pub exit: Option<i32>,
    /// Set when the program called `fail`: `msg` is the message it gave. `try` catches these.
    pub failure: bool,
    /// The source line the error points to, with a mark under the column (see `Loaded`).
    pub snippet: Option<String>,
}

impl Error {
    pub fn new(msg: impl Into<String>, line: usize, col: usize) -> Self {
        Error {
            msg: msg.into(),
            line,
            col,
            file: None,
            exit: None,
            failure: false,
            snippet: None,
        }
    }

    pub fn runtime(msg: impl Into<String>) -> Self {
        Error::new(msg, 0, 0)
    }

    /// What `exit` raises to stop the interpreter with an exit status.
    pub fn exit(code: i32) -> Self {
        let mut e = Error::runtime(format!("exit with status {}", code));
        e.exit = Some(code);
        e
    }

    /// What `fail` raises: a failure with a message, which `try` can catch.
    pub fn failure(msg: impl Into<String>) -> Self {
        let mut e = Error::runtime(msg);
        e.failure = true;
        e
    }

    /// The same error, located in `file`.
    pub fn in_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.failure {
            write!(f, "error: uncaught failure: {}", self.msg)
        } else if self.line == 0 {
            write!(f, "error: {}", self.msg)
        } else {
            match &self.file {
                Some(file) => write!(
                    f,
                    "error in {} at {}:{}: {}",
                    file, self.line, self.col, self.msg
                ),
                None => write!(f, "error at {}:{}: {}", self.line, self.col, self.msg),
            }
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_with_and_without_a_file() {
        assert_eq!(Error::new("boom", 2, 3).to_string(), "error at 2:3: boom");
        assert_eq!(
            Error::new("boom", 2, 3).in_file("a/b.stk").to_string(),
            "error in a/b.stk at 2:3: boom"
        );
        assert_eq!(Error::runtime("boom").to_string(), "error: boom");
    }

    #[test]
    fn exit_carries_a_status() {
        assert_eq!(Error::exit(3).exit, Some(3));
        assert_eq!(Error::runtime("boom").exit, None);
    }

    #[test]
    fn a_failure_keeps_its_message() {
        let e = Error::failure("boom");
        assert!(e.failure);
        assert_eq!(e.msg, "boom");
        assert_eq!(e.to_string(), "error: uncaught failure: boom");
        assert!(!Error::runtime("boom").failure);
    }
}
