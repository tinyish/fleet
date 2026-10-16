//! How a command ends.

/// How one command ended, as a person or a script sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// It did what was asked; the text says what it found or did.
    Done(String),
    /// It did not do it, and knows it did not; the text says why.
    Refused(String),
    /// It needs the supervisor, and the supervisor is not running.
    SupervisorNotRunning,
    /// It cannot tell whether it happened; the text says what it saw.
    CouldNotTell(String),
}

impl Outcome {
    /// The exit status a script reads for this outcome.
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        match self {
            Outcome::Done(_) => 0,
            Outcome::Refused(_) => 1,
            Outcome::SupervisorNotRunning => 3,
            Outcome::CouldNotTell(_) => 4,
        }
    }
}
