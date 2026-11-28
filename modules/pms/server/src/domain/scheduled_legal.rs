#[derive(Clone, Copy, Debug)]
pub enum ScheduledReleaseMode {
    Development,
    Production,
}
pub fn scheduled_legal_allowed(
    mode: ScheduledReleaseMode,
    rollout: Option<&str>,
    accepted: bool,
    engineering: bool,
) -> bool {
    match mode {
        ScheduledReleaseMode::Development => true,
        ScheduledReleaseMode::Production => {
            engineering || matches!(rollout, Some("pilot" | "available")) && accepted
        }
    }
}
