use crate::config::Config;

pub fn cmd_doctor() -> anyhow::Result<()> {
    let config = Config::find_and_load().ok();
    let report = crate::doctor::run_all_checks(config.as_ref());
    crate::doctor::print_report(&report);

    if report.has_failures() {
        std::process::exit(1);
    }
    Ok(())
}
