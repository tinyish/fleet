//! tinyfleet's look for prompts and spinners: cliclack, restyled after Vite+.
//! Frameless (no guide bars down the left), a blue `›` for what is active, a
//! gray `◇` once a step is answered, a magenta spinner, yellow for warnings
//! and red for errors. On a true-colour terminal the blue is Vite+'s own
//! (#5892FF); elsewhere it is the terminal's. The spinner is always the
//! terminal's magenta, since indicatif's template takes named colours only
//! (Vite+'s true-colour magenta is #BB74F7).

use std::fmt::Write;

use cliclack::{Theme, ThemeState};
use console::{Style, style};

use crate::words;

/// Vite+'s blue, `rgb(88, 146, 255)`.
const BLUE: (u8, u8, u8) = (88, 146, 255);

/// The colours the theme draws with.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Tone {
    /// What is active or worth noticing: pointers, names, commands.
    Accent,
    /// What is done or secondary.
    Muted,
    /// Something went well.
    Good,
    /// Something to look at.
    Warn,
    /// Something went wrong.
    Bad,
}

/// The style for `tone` on stderr, where prompts and spinners draw.
pub(crate) fn paint(tone: Tone) -> Style {
    paint_on(tone, true)
}

/// The style for `tone` on stdout or stderr, in true colour when that stream
/// supports it. Colour itself is only on when the stream is a terminal.
pub(crate) fn paint_on(tone: Tone, stderr: bool) -> Style {
    let (style, rgb) = if stderr {
        (
            Style::new().for_stderr(),
            console::true_colors_enabled_stderr(),
        )
    } else {
        (Style::new().for_stdout(), console::true_colors_enabled())
    };
    match tone {
        Tone::Accent if rgb => style.true_color(BLUE.0, BLUE.1, BLUE.2),
        Tone::Accent => style.blue(),
        Tone::Muted => style.black().bright(),
        Tone::Good => style.green(),
        Tone::Warn => style.yellow(),
        Tone::Bad => style.red(),
    }
}

const POINTER: &str = ">";
const STEP_SUBMIT: &str = "◇";
const STEP_CANCEL: &str = "■";
const STEP_ERROR: &str = "▲";
const INFO: &str = "●";
pub(crate) const SUCCESS: &str = "◆";
pub(crate) const WARN: &str = "▲";
pub(crate) const ERROR: &str = "■";
pub(crate) const NEXT: &str = ">";

/// The theme installed for every prompt and spinner.
pub(crate) struct FleetTheme;

impl FleetTheme {
    fn indent(text: &str, first: &str) -> String {
        let mut out = String::new();
        for (i, line) in text.lines().enumerate() {
            let prefix = if i == 0 { first } else { " " };
            let _ = writeln!(out, "{prefix} {line}");
        }
        out
    }
}

impl Theme for FleetTheme {
    fn bar_color(&self, state: &ThemeState) -> Style {
        match state {
            ThemeState::Active => paint(Tone::Accent),
            ThemeState::Cancel => paint(Tone::Bad),
            ThemeState::Error(_) => paint(Tone::Warn),
            ThemeState::Submit => paint(Tone::Muted),
        }
    }

    fn state_symbol(&self, state: &ThemeState) -> String {
        match state {
            ThemeState::Active => paint(Tone::Accent).apply_to(POINTER).to_string(),
            ThemeState::Submit => paint(Tone::Muted).apply_to(STEP_SUBMIT).to_string(),
            ThemeState::Cancel => paint(Tone::Bad).apply_to(STEP_CANCEL).to_string(),
            ThemeState::Error(_) => paint(Tone::Warn).apply_to(STEP_ERROR).to_string(),
        }
    }

    fn info_symbol(&self) -> String {
        paint(Tone::Accent).apply_to(INFO).to_string()
    }

    fn warning_symbol(&self) -> String {
        paint(Tone::Warn).apply_to(WARN).to_string()
    }

    fn error_symbol(&self) -> String {
        paint(Tone::Bad).apply_to(ERROR).to_string()
    }

    fn submit_symbol(&self) -> String {
        paint(Tone::Good).apply_to(SUCCESS).to_string()
    }

    fn active_symbol(&self) -> String {
        paint(Tone::Accent).apply_to(POINTER).to_string()
    }

    fn format_header(&self, state: &ThemeState, prompt: &str) -> String {
        Self::indent(prompt, &self.state_symbol(state))
    }

    fn format_footer_with_message(&self, state: &ThemeState, message: &str) -> String {
        match state {
            ThemeState::Active if message.is_empty() => String::new(),
            ThemeState::Active => format!("  {}\n", style(message).dim()),
            ThemeState::Submit => String::new(),
            ThemeState::Cancel => {
                format!("  {}\n", paint(Tone::Bad).apply_to(words::CANCELLED))
            }
            ThemeState::Error(err) => format!("  {}\n", paint(Tone::Warn).apply_to(err)),
        }
    }

    fn format_confirm(&self, state: &ThemeState, confirm: bool) -> String {
        let chosen = if confirm { words::YES } else { words::NO };
        match state {
            ThemeState::Submit => format!("  {}\n", style(chosen).dim()),
            ThemeState::Cancel => format!("  {}\n", style(chosen).dim().strikethrough()),
            _ => {
                let option = |label: &str, on: bool| {
                    if on {
                        format!(
                            "{} {}",
                            paint(Tone::Accent).apply_to(POINTER),
                            style(label).bold()
                        )
                    } else {
                        format!("  {}", style(label).dim())
                    }
                };
                format!(
                    "  {} {} {}\n",
                    option(words::YES, confirm),
                    style("/").dim(),
                    option(words::NO, !confirm)
                )
            }
        }
    }

    fn format_log_with_spacing(&self, text: &str, symbol: &str, _spacing: bool) -> String {
        Self::indent(text, symbol)
    }

    fn format_progress_start(&self, template: &str, _grouped: bool, _last: bool) -> String {
        // The spinner key takes indicatif's named colours only, so it is the
        // terminal's magenta even on a true-colour terminal.
        format!("{{spinner:.magenta}} {template} ({{elapsed:.dim}})")
    }

    fn format_progress_with_state(
        &self,
        msg: &str,
        _grouped: bool,
        _last: bool,
        state: &ThemeState,
    ) -> String {
        if msg.is_empty() {
            return String::new();
        }
        match state {
            ThemeState::Active => msg.to_owned(),
            _ => format!("{} {msg}", self.state_symbol(state)),
        }
    }
}
