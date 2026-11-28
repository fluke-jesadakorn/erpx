//! First step of Records requestAccess. This evaluates no authorization.
use erpx_workflow_server::public::source_ordered_json::{SourceOrderedJson, SourceOrderedValue};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RfqDesignatedStep {
    None,
    DepartmentPolicy,
    JsonSyntax,
    NullProperty,
    PropertyKeyTypeError,
    RuntimeCoercionUnproven,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RfqPriorReviewer {
    Yes,
    No,
    JsonSyntax,
    PropertyTypeError,
}

/// The last requestAccess fallback after own/owner/member/approver are false.
/// Preserve `decisions?.some` short circuit, including null element errors.
pub(crate) fn rfq_prior_reviewer(
    approval_json: &str,
    same_person: impl Fn(&str) -> bool,
) -> RfqPriorReviewer {
    use RfqPriorReviewer::*;
    let approval = match SourceOrderedJson::parse(approval_json) {
        Ok(value) => value,
        Err(_) => return JsonSyntax,
    };
    let saved = approval.root();
    if saved.kind() == "null" {
        return PropertyTypeError;
    }
    if let Some(decisions) = saved.property("decisions").filter(|v| v.kind() != "null") {
        let Some(len) = decisions.array_len() else {
            return PropertyTypeError;
        };
        for index in 0..len {
            let entry = decisions.element(index).expect("array index");
            if entry.kind() == "null" {
                return PropertyTypeError;
            }
            if let Some(user) = entry.property("userId").and_then(|v| v.utf16()) {
                if String::from_utf16(user)
                    .ok()
                    .is_some_and(|id| same_person(&id))
                {
                    return Yes;
                }
            }
        }
    }
    if saved
        .property("decidedBy")
        .and_then(|v| v.utf16())
        .and_then(|s| String::from_utf16(s).ok())
        .is_some_and(|id| same_person(&id))
    {
        Yes
    } else {
        No
    }
}

fn property_key(value: Option<SourceOrderedValue<'_>>) -> Result<Vec<u16>, RfqDesignatedStep> {
    let Some(value) = value else {
        return Ok(vec![b'0' as u16]);
    };
    if value.kind() == "null" {
        return Ok(vec![b'0' as u16]);
    }
    enum Part<'a> {
        Value(SourceOrderedValue<'a>, bool),
        Comma,
    }
    let mut output = Vec::new();
    let mut work = vec![Part::Value(value, false)];
    while let Some(part) = work.pop() {
        let Part::Value(value, inside_array) = part else {
            output.push(b',' as u16);
            continue;
        };
        match value.kind() {
            "null" => {} // Array.prototype.join renders null as an empty field.
            "string" => output.extend_from_slice(value.utf16().expect("string kind")),
            "number" => {
                let number = value.number().expect("number kind");
                let text = if number == f64::INFINITY {
                    "Infinity".to_owned()
                } else if number == f64::NEG_INFINITY {
                    "-Infinity".to_owned()
                } else {
                    value.stringify()
                };
                output.extend(text.encode_utf16());
            }
            "boolean" => output.extend(value.stringify().encode_utf16()),
            "array" => {
                // Node uses recursive Array#join and may raise RangeError at
                // environment-dependent depths. Do not replace that outcome
                // with an invented numeric limit or a successful decision.
                if inside_array {
                    return Err(RfqDesignatedStep::RuntimeCoercionUnproven);
                }
                let len = value.array_len().expect("array kind");
                for index in (0..len).rev() {
                    work.push(Part::Value(
                        value.element(index).expect("array index"),
                        true,
                    ));
                    if index > 0 {
                        work.push(Part::Comma);
                    }
                }
            }
            "object" => {
                // JSON cannot carry callable values. An own toString shadows
                // Object.prototype.toString and valueOf still returns an
                // object (or is also noncallable), so coercion throws.
                if value.property("toString").is_some() {
                    return Err(RfqDesignatedStep::PropertyKeyTypeError);
                }
                output.extend("[object Object]".encode_utf16());
            }
            _ => unreachable!("JSON value kind"),
        }
    }
    Ok(output)
}

/// Mirrors `saved.route?.steps[saved.currentStep ?? 0]` through the
/// designated step's truthiness/samePerson checks. A policy result still
/// requires the original snapshot and Organization's departmentStepEligible.
pub(crate) fn rfq_designated_step(
    approval_json: &str,
    same_person: impl Fn(&str) -> bool,
) -> RfqDesignatedStep {
    use RfqDesignatedStep::*;
    let approval = match SourceOrderedJson::parse(approval_json) {
        Ok(value) => value,
        Err(_) => return JsonSyntax,
    };
    let saved = approval.root();
    if saved.kind() == "null" {
        return NullProperty;
    }
    let Some(route) = saved.property("route") else {
        return None;
    };
    if route.kind() == "null" {
        return None;
    }
    let Some(steps) = route.property("steps") else {
        return NullProperty;
    };
    if steps.kind() == "null" {
        return NullProperty;
    }
    let key = match property_key(saved.property("currentStep")) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let current = if steps.kind() == "object" {
        steps.property_utf16(&key)
    } else if steps.kind() == "array" {
        let index = String::from_utf16(&key).ok().and_then(|s| {
            let n = s.parse::<usize>().ok()?;
            (n.to_string() == s).then_some(n)
        });
        index.and_then(|n| steps.element(n))
    } else {
        // Indexing a non-null primitive yields either a boxed primitive or
        // undefined, neither can supply the snapshot property of interest.
        Option::None
    };
    let Some(current) = current else {
        return None;
    };
    if current.kind() == "null" {
        return None;
    }
    let Some(snapshot) = current.property("snapshot") else {
        return None;
    };
    if snapshot.kind() == "null" {
        return None;
    }
    if !snapshot
        .property("departmentPolicyVersion")
        .is_some_and(|v| v.truthy())
    {
        return None;
    }
    let Some(user) = current.property("userId").and_then(|v| v.utf16()) else {
        return None;
    };
    let Ok(user) = String::from_utf16(user) else {
        return None;
    };
    if !same_person(&user) {
        return None;
    }
    DepartmentPolicy
}
