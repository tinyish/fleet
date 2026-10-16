//! tinyfleet's look for prompts and spinners: cliclack, restyled after Vite+.
//! Frameless (no guide bars down the left), a blue `>` for what is active, a
//! gray `◇` once a step is answered, a magenta spinner, yellow for warnings
//! and red for errors. On a true-colour terminal the colours are the
//! palette's (Tailwind's sky, green, slate, red and yellow); elsewhere they
//! are the terminal's own. The spinner is always the terminal's magenta,
//! since indicatif's template takes named colours only.

use std::fmt::Write;

use cliclack::{Theme, ThemeState};
use console::{Style, style};

use crate::words;

/// The palette, on a true-colour terminal: Tailwind's sky-400, green-400,
/// slate-500, red-400 and yellow-300.
const ACCENT: (u8, u8, u8) = (0x38, 0xBD, 0xF8);
const GOOD: (u8, u8, u8) = (0x4A, 0xDE, 0x80);
const MUTED: (u8, u8, u8) = (0x64, 0x74, 0x8B);
const BAD: (u8, u8, u8) = (0xF8, 0x71, 0x71);
const WARN_YELLOW: (u8, u8, u8) = (0xFD, 0xE0, 0x47);

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
    let rgb_of = |(r, g, b): (u8, u8, u8)| style.clone().true_color(r, g, b);
    match tone {
        Tone::Accent if rgb => rgb_of(ACCENT),
        Tone::Good if rgb => rgb_of(GOOD),
        Tone::Muted if rgb => rgb_of(MUTED),
        Tone::Bad if rgb => rgb_of(BAD),
        Tone::Warn if rgb => rgb_of(WARN_YELLOW),
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
