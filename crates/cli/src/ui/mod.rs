//! The terminal's touches: a yes/no prompt, a spinner and colour, drawn with
//! cliclack in tinyfleet's theme. Each one shows only on a terminal, so a
//! script reads plain lines.

pub(crate) mod output;
pub(crate) mod theme;
pub(crate) mod words;

use std::sync::Once;

use cliclack::ProgressBar;

use crate::ui::theme::FleetTheme;

static THEME: Once = Once::new();

fn themed() {
    THEME.call_once(|| cliclack::set_theme(FleetTheme));
}

/// Asks a yes/no question on the terminal. Enter takes `default`; Esc,
/// Ctrl-C, a closed terminal or an error is `None`, a cancel, and each caller
/// says what a cancel means.
pub(crate) fn confirm(question: &str, default: bool) -> Option<bool> {
    themed();
    cliclack::confirm(question)
        .initial_value(default)
        .interact()
        .ok()
}

/// A spinner on stderr, drawn only when stderr is a terminal. `clear` it when
/// the wait is over.
pub(crate) fn spinner(message: &'static str) -> ProgressBar {
    themed();
    let spinner = cliclack::spinner();
    spinner.start(message);
    spinner
}
