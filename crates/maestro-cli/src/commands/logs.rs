use crate::session;

pub fn cmd_logs(session: Option<String>, export: Option<String>) -> anyhow::Result<()> {
    let logger = session::logger::SessionLogger::new(session::logger::SessionLogger::default_dir());

    if let Some(session_id_str) = session {
        let session_id: uuid::Uuid = session_id_str
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid session ID: {}", session_id_str))?;
        let content = logger.read_log(session_id)?;

        if export.as_deref() == Some("json") {
            let lines: Vec<&str> = content.lines().collect();
            let json = serde_json::json!({
                "session_id": session_id_str,
                "lines": lines,
            });
            println!("{}", serde_json::to_string_pretty(&json)?);
        } else {
            println!("{}", render_log(&content));
        }
    } else {
        let logs = logger.list_logs()?;
        if logs.is_empty() {
            println!("No session logs found.");
            return Ok(());
        }

        if export.as_deref() == Some("json") {
            let entries: Vec<serde_json::Value> = logs
                .iter()
                .map(|l| {
                    serde_json::json!({
                        "session_id": l.session_id,
                        "size_bytes": l.size_bytes,
                        "path": l.path.display().to_string(),
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&entries)?);
        } else {
            println!("{:<40} {:>10}", "Session ID", "Size");
            println!("{}", "-".repeat(52));
            for log in &logs {
                println!(
                    "{:<40} {:>10}",
                    log.session_id,
                    format_bytes(log.size_bytes)
                );
            }
            println!("\n{} log(s) found.", logs.len());
        }
    }
    Ok(())
}

/// Strips ANSI/OSC escapes and C0 controls (except LF) from a persisted log
/// before it reaches the user's terminal. Logs keep raw agent output, so an
/// OSC 52 or cursor sequence would otherwise replay on `maestro logs` (#1043).
/// Tabs and CR are dropped too; the JSON export keeps the raw bytes.
fn render_log(content: &str) -> String {
    crate::util::sanitize::strip_ansi(content)
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OSC52_AND_CSI: &str = "[12:00:00] ASSISTANT: hello \x1b]52;c;ZXZpbA==\x07world\n\
                                 [12:00:01] UNKNOWN: \x1b[2J\x1b[31mred\x1b[0m done\n";

    #[test]
    fn render_log_strips_osc52_and_csi_escapes() {
        let out = render_log(OSC52_AND_CSI);
        assert!(!out.contains('\x1b'), "ESC byte leaked: {out:?}");
        assert!(!out.contains('\x07'), "BEL byte leaked: {out:?}");
    }

    #[test]
    fn render_log_keeps_plain_text_and_newlines() {
        let out = render_log(OSC52_AND_CSI);
        assert!(out.contains("[12:00:00] ASSISTANT: hello world"), "{out:?}");
        assert!(out.contains("[12:00:01] UNKNOWN: red done"), "{out:?}");
        assert_eq!(out.lines().count(), 2);
    }

    #[test]
    fn render_log_plain_log_unchanged() {
        let plain = "[12:00:00] ASSISTANT: plain text\n[12:00:01] DONE\n";
        assert_eq!(render_log(plain), plain);
    }
}
