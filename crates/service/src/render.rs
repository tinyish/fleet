//! The supervisor's unit files, as text. Pure: nothing here touches disk.

use std::path::{Path, PathBuf};

use fleet_core::tools::{SupervisorUnit, ToolError};

fn text<'a>(path: &'a Path, what: &str) -> Result<&'a str, ToolError> {
    let s = path.to_str().ok_or_else(|| {
        ToolError(format!(
            "the {what} path is not valid UTF-8: {}",
            path.display()
        ))
    })?;
    if s.contains('\n') {
        return Err(ToolError(format!("the {what} path contains a newline")));
    }
    Ok(s)
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Quotes `s` for a systemd unit line. `ExecStart=` expands `$`, so there
/// it is doubled; `Environment=` does not expand it.
fn systemd_quoted(s: &str, dollar: bool) -> String {
    let mut inner = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%");
    if dollar {
        inner = inner.replace('$', "$$");
    }
    format!("\"{inner}\"")
}

fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The state directory an installed launchd property list serves, if it
/// names one.
#[must_use]
pub fn plist_state_dir(text: &str) -> Option<PathBuf> {
    let after = text.split_once("<key>FLEET_STATE_DIR</key>")?.1;
    let rest = after.split_once("<string>")?.1;
    let value = rest.split_once("</string>")?.0;
    Some(PathBuf::from(xml_unescape(value)))
}

/// The state directory an installed systemd unit serves, if it names one.
#[must_use]
pub fn systemd_state_dir(text: &str) -> Option<PathBuf> {
    let quoted = text
        .lines()
        .find_map(|l| l.strip_prefix("Environment=\"FLEET_STATE_DIR="))?
        .strip_suffix('"')?;
    let mut out = String::new();
    let mut chars = quoted.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' | '%' => out.push(chars.next()?),
            _ => out.push(c),
        }
    }
    Some(PathBuf::from(out))
}

/// The launchd property list for the supervisor.
///
/// # Errors
///
/// When a path is not UTF-8 or contains a newline.
pub fn plist(unit: &SupervisorUnit, log: &Path) -> Result<String, ToolError> {
    let program = xml(text(&unit.program, "program")?);
    let state_dir = xml(text(&unit.state_dir, "state directory")?);
    let log = xml(text(log, "log")?);
    let label = crate::LAUNCHD_LABEL;
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">
<plist version=\"1.0\">
<dict>
\t<key>Label</key>
\t<string>{label}</string>
\t<key>ProgramArguments</key>
\t<array>
\t\t<string>{program}</string>
\t\t<string>supervise</string>
\t</array>
\t<key>EnvironmentVariables</key>
\t<dict>
\t\t<key>FLEET_STATE_DIR</key>
\t\t<string>{state_dir}</string>
\t</dict>
\t<key>RunAtLoad</key>
\t<true/>
\t<key>KeepAlive</key>
\t<dict>
\t\t<key>SuccessfulExit</key>
\t\t<false/>
\t</dict>
\t<key>StandardOutPath</key>
\t<string>{log}</string>
\t<key>StandardErrorPath</key>
\t<string>{log}</string>
</dict>
</plist>
"
    ))
}

/// The systemd user unit for the supervisor.
///
/// # Errors
///
/// When a path is not UTF-8 or contains a newline.
pub fn systemd_unit(unit: &SupervisorUnit) -> Result<String, ToolError> {
    let program = systemd_quoted(text(&unit.program, "program")?, true);
    let env = systemd_quoted(
        &format!(
            "FLEET_STATE_DIR={}",
            text(&unit.state_dir, "state directory")?
        ),
        false,
    );
    Ok(format!(
        "[Unit]
Description=tinyfleet supervisor

[Service]
ExecStart={program} supervise
Environment={env}
Restart=on-failure

[Install]
WantedBy=default.target
"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(program: &str, state_dir: &str) -> SupervisorUnit {
        SupervisorUnit {
            program: PathBuf::from(program),
            state_dir: PathBuf::from(state_dir),
        }
    }

    #[test]
    fn plist_escapes_xml_and_keeps_spaces() {
        let u = unit("/a b/fl&et", "/Users/x/Library/Application Support/fleet");
        let text = plist(
            &u,
            Path::new("/Users/x/Library/Application Support/fleet/supervisor.log"),
        )
        .unwrap_or_default();
        assert!(text.contains("<string>/a b/fl&amp;et</string>"));
        assert!(text.contains("<string>supervise</string>"));
        assert!(text.contains("<string>/Users/x/Library/Application Support/fleet</string>"));
        assert!(text.contains("<key>Label</key>\n\t<string>com.tinyish.fleet.supervisor</string>"));
        assert!(text.contains("<key>RunAtLoad</key>\n\t<true/>"));
    }

    #[test]
    fn systemd_quotes_spaces_quotes_backslashes_percent_and_dollar() {
        let u = unit("/opt/my \"fl\\eet\"/fleet", "/home/a/50%/$x");
        let text = systemd_unit(&u).unwrap_or_default();
        assert!(text.contains("ExecStart=\"/opt/my \\\"fl\\\\eet\\\"/fleet\" supervise\n"));
        assert!(text.contains("Environment=\"FLEET_STATE_DIR=/home/a/50%%/$x\"\n"));
        assert!(text.contains("WantedBy=default.target\n"));
        assert!(text.contains("Restart=on-failure\n"));
    }

    #[test]
    fn the_installed_state_dir_reads_back_for_odd_paths() {
        for dir in [
            "/Users/x/Library/Application Support/fleet",
            "/a&b<c>'\"d",
            "/home/a/50%/$x",
            "/back\\slash",
        ] {
            let u = unit("/bin/fleet", dir);
            let p = plist(&u, Path::new("/s/l")).unwrap_or_default();
            assert_eq!(plist_state_dir(&p), Some(PathBuf::from(dir)), "plist {dir}");
            let s = systemd_unit(&u).unwrap_or_default();
            assert_eq!(
                systemd_state_dir(&s),
                Some(PathBuf::from(dir)),
                "systemd {dir}"
            );
        }
    }

    #[test]
    fn a_unit_without_the_variable_has_no_state_dir() {
        assert_eq!(plist_state_dir("<plist><dict></dict></plist>"), None);
        assert_eq!(systemd_state_dir("[Service]\nExecStart=/bin/true\n"), None);
    }

    #[test]
    fn a_newline_in_a_path_is_refused() {
        assert!(systemd_unit(&unit("/a\nb", "/s")).is_err());
        assert!(plist(&unit("/a\nb", "/s"), Path::new("/s/l")).is_err());
    }
}
