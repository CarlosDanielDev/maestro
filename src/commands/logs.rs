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
            println!("{}", content);
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

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
