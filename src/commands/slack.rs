use crate::commands::setup::build_notification_dispatcher;
use crate::config::Config;

pub async fn cmd_test_slack() -> anyhow::Result<()> {
    let config = Config::find_and_load()?;
    let mut dispatcher = build_notification_dispatcher(&config.notifications);

    if !dispatcher.has_slack() {
        anyhow::bail!(
            "Slack is not configured. Set notifications.slack = true and slack_webhook_url in maestro.toml"
        );
    }

    println!("Sending test message to Slack webhook...");
    match dispatcher.test_slack().await {
        Ok(true) => {
            println!("Slack webhook test successful!");
            Ok(())
        }
        Ok(false) => {
            anyhow::bail!("Test was rate-limited. Try again later.")
        }
        Err(e) => {
            anyhow::bail!("Slack webhook test failed: {}", e)
        }
    }
}
