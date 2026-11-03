//! PMS source disclosure policy, supplied with current selected permissions.
pub fn source_allowed(
    kind: &str,
    user: &str,
    requested_by: Option<&str>,
    recorded_by: Option<&str>,
    owner: Option<&str>,
    can: impl Fn(&str) -> bool,
) -> bool {
    if !can("pms.read") {
        return false;
    }
    match kind {
        "room" | "task" | "block" => {
            can("pms.housekeeping") || can("pms.inspect") || can("pms.maintenance")
        }
        "stay" | "reservation" => {
            can("pms.frontdesk") || can("pms.reservations") || can("pms.cashier")
        }
        "guest_account" => can("pms.cashier") || can("pms.reports"),
        "refund" | "correction" => {
            can("pms.approve") || can("pms.cashier") && requested_by == Some(user)
        }
        "transfer" => can("finance.approve") || can("pms.cashier") && recorded_by == Some(user),
        "shift" => {
            can("pms.approve") || can("pms.audit") || can("pms.cashier") && owner == Some(user)
        }
        "night" => can("pms.audit") || can("pms.reports"),
        _ => false,
    }
}
pub fn source_title(property: &str, kind: &str, number: Option<&str>) -> Option<String> {
    let label = match kind {
        "room" => "Room",
        "task" => "Housekeeping task",
        "block" => "Room block",
        "stay" => "Stay",
        "reservation" => "Reservation",
        "guest_account" => "Guest Account",
        "refund" => "Refund",
        "correction" => "Correction",
        "transfer" => "Transfer",
        "shift" => "Shift",
        "night" => "Night audit",
        _ => return None,
    };
    Some(format!(
        "{property} · {label}{}",
        number
            .filter(|v| !v.is_empty())
            .map(|v| format!(" {v}"))
            .unwrap_or_default()
    ))
}
