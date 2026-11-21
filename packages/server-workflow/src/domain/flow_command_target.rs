//! Pure Workflow command-to-flow routing classification. This is a selector
//! only; the caller must still resolve current IAM/purchase/record authority.
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlowCommandTarget {
    pub kind: String,
    pub id: String,
    pub action: String,
}

const EXCLUDED_OPERATIONS: &[&str] = &[
    "record.archive",
    "record.mention",
    "record.reference",
    "record.suggestion",
    "record.suggestion.decide",
];
const EXCLUDED_ACTIONS: &[&str] = &[
    "create",
    "update",
    "revise",
    "attachment.remove",
    "ai",
    "post.reverse",
    "recalculate",
    "review-export",
    "schedule",
    "delegate",
    "delegate-revoke",
    "reverse",
];
const EXCLUDED_PREFIXES: &[&str] = &[
    "ai.",
    "payment.",
    "mapping.",
    "correction.",
    "adjustment.",
    "final-pay.",
    "advance.",
    "accrual.",
];

const RULES: &[(&str, &str)] = &[
    ("expense.", "expense"),
    ("leave.", "leave"),
    ("task.", "staff_task"),
    ("employee.request.", "request"),
    ("inventory.count.", "stock_count"),
    ("inventory.transfer.", "transfer"),
    ("commerce.order.", "order"),
    ("commerce.invoice.settle", "invoice"),
    ("finance.invoice.post", "invoice"),
    ("finance.journal.post", "journal"),
    ("people.payroll.", "payroll"),
    ("record.", "record"),
];

fn excluded_action(action: &str) -> bool {
    EXCLUDED_ACTIONS.contains(&action)
        || EXCLUDED_PREFIXES
            .iter()
            .any(|prefix| action.starts_with(prefix))
}

/// Mirror `flowCommandTarget(operation,id,input)`. No label or HTTP data is
/// trusted as authority; an unknown operation returns `None`.
pub fn flow_command_target(
    operation: &str,
    id: &str,
    input: Option<&Value>,
) -> Option<FlowCommandTarget> {
    if operation.starts_with("leave.self.")
        || operation.starts_with("leave.policy.")
        || operation.starts_with("leave.cancellation")
        || EXCLUDED_OPERATIONS.contains(&operation)
    {
        return None;
    }
    if operation == "finance.service-no-charge.confirm" {
        return Some(FlowCommandTarget {
            kind: "invoice".into(),
            id: id.into(),
            action: "post".into(),
        });
    }
    let (prefix, kind) = RULES
        .iter()
        .find(|(prefix, _)| operation.starts_with(prefix))?;
    let mut action = if prefix.ends_with('.') {
        operation[prefix.len()..].to_owned()
    } else {
        operation
            .rsplit_once('.')
            .map_or_else(|| operation.to_owned(), |(_, action)| action.to_owned())
    };
    if action == "transition"
        && let Some(Value::Object(values)) = input
        && let Some(Value::String(next)) = values.get("action")
    {
        action = next.clone();
    }
    if excluded_action(&action) {
        return None;
    }
    Some(FlowCommandTarget {
        kind: (*kind).into(),
        id: id.into(),
        action,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;
    use std::io::Write;

    #[test]
    fn routing_matches_actual_typescript_classification() {
        let cases = json!([
            ["expense.submit", "e1", null],
            ["expense.transition", "e1", {"action":"approve"}],
            ["leave.submit", "l1", null],
            ["leave.self.submit", "l1", null],
            ["leave.cancellation.approve", "l1", null],
            ["task.transition", "t1", {"action":"complete"}],
            ["employee.request.submit", "r1", null],
            ["inventory.count.approve", "c1", null],
            ["inventory.transfer.dispatch", "x1", null],
            ["commerce.order.fulfill", "o1", null],
            ["commerce.invoice.settle", "i1", null],
            ["finance.invoice.post", "i1", null],
            ["finance.journal.post", "j1", null],
            ["finance.service-no-charge.confirm", "i1", null],
            ["people.payroll.approve", "p1", null],
            ["record.transition", "rec1", {"action":"submit"}],
            ["record.reference", "rec1", null],
            ["record.suggestion.decide", "rec1", null],
            ["commerce.order.create", "o1", null],
            ["commerce.order.payment.capture", "o1", null],
            ["unknown.operation", "u1", null]
        ]);
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let module = root.join("packages/server-workflow/src/domain/flow-routing.ts");
        let script = r#"const m=await import(process.argv[1]);let raw='';for await(const c of process.stdin)raw+=c;const cases=JSON.parse(raw);process.stdout.write(JSON.stringify(cases.map(([operation,id,input])=>m.flowCommandTarget(operation,id,input))));"#;
        let mut child =
            std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required"))
                .current_dir(root)
                .args(["--conditions=react-server", "-e", script])
                .arg(module)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
                .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(cases.to_string().as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "actual TS flow routing oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected: Vec<Option<FlowCommandTargetWire>> =
            serde_json::from_slice(&output.stdout).unwrap();
        let actual: Vec<Option<FlowCommandTarget>> = cases
            .as_array()
            .unwrap()
            .iter()
            .map(|case| {
                flow_command_target(
                    case[0].as_str().unwrap(),
                    case[1].as_str().unwrap(),
                    case.get(2).filter(|value| !value.is_null()),
                )
            })
            .collect();
        let actual_wire: Vec<Option<FlowCommandTargetWire>> = actual
            .into_iter()
            .map(|target| {
                target.map(|target| FlowCommandTargetWire {
                    kind: target.kind,
                    id: target.id,
                    action: target.action,
                })
            })
            .collect();
        assert_eq!(actual_wire, expected);
    }

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct FlowCommandTargetWire {
        kind: String,
        id: String,
        action: String,
    }
}
