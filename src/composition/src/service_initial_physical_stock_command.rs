//! Strict Service initial Count wire normalization for existing Workflow receipt
//! identity. JSON is canonicalized with the retained ECMAScript codec.
use erpx_server_inventory::public::service_initial_physical_count::InitialCountLot;
use erpx_server_shared::public::BusinessError;
use erpx_server_workflow::public::commands::CommandJson;
fn invalid() -> BusinessError {
    BusinessError::invalid(
        "VALIDATION_FAILED",
        "Use valid initial physical Count input.",
    )
}
fn strict(v: &CommandJson, allowed: &[&str]) -> Result<(), BusinessError> {
    if v.object_keys()?
        .ok_or_else(invalid)?
        .iter()
        .any(|k| !allowed.contains(&k.as_str()))
    {
        return Err(invalid());
    }
    Ok(())
}
fn prop(v: &CommandJson, key: &str) -> Result<CommandJson, BusinessError> {
    v.property(key)?.ok_or_else(invalid)
}
fn text(
    v: &CommandJson,
    min: usize,
    max: usize,
    trim: bool,
) -> Result<(String, CommandJson), BusinessError> {
    let mut u = v.string_utf16_units()?.ok_or_else(invalid)?;
    if trim {
        let white = |c: &u16| matches!(*c,0x0009..=0x000d|0x0020|0x00a0|0x1680|0x2000..=0x200a|0x2028|0x2029|0x202f|0x205f|0x3000|0xfeff);
        let a = u.iter().position(|c| !white(c)).unwrap_or(u.len());
        let b = u.iter().rposition(|c| !white(c)).map_or(a, |i| i + 1);
        u = u[a..b].to_vec();
    }
    if u.len() < min || u.len() > max {
        return Err(invalid());
    }
    Ok((
        String::from_utf16_lossy(&u),
        CommandJson::from_utf16_string(&u)?,
    ))
}
fn decimal(v: &CommandJson, scale: usize) -> Result<(i64, CommandJson), BusinessError> {
    let (s, j) = text(v, 1, 16, false)?;
    let mut parts = s.split('.');
    let whole = parts.next().ok_or_else(invalid)?;
    let frac = parts.next();
    if parts.next().is_some()
        || whole.is_empty()
        || whole.len() > 12
        || !whole.bytes().all(|c| c.is_ascii_digit())
        || frac.is_some_and(|f| {
            f.is_empty() || f.len() > scale || !f.bytes().all(|c| c.is_ascii_digit())
        })
    {
        return Err(invalid());
    }
    let major = whole.parse::<i64>().map_err(|_| invalid())?;
    let f = frac.unwrap_or("");
    let minor = if f.is_empty() {
        0
    } else {
        f.parse::<i64>().map_err(|_| invalid())? * 10i64.pow((scale - f.len()) as u32)
    };
    Ok((major * 10i64.pow(scale as u32) + minor, j))
}
fn date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return false;
    }
    let (Ok(y), Ok(m), Ok(d)) = (
        s[..4].parse::<u32>(),
        s[5..7].parse::<u32>(),
        s[8..].parse::<u32>(),
    ) else {
        return false;
    };
    let max = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) => 29,
        2 => 28,
        _ => return false,
    };
    d > 0 && d <= max
}
fn number(v: &CommandJson) -> Result<i64, BusinessError> {
    let n = v.as_str().parse::<f64>().map_err(|_| invalid())?;
    if !n.is_finite() || n.fract() != 0.0 || !(0.0..=9_007_199_254_740_991.0).contains(&n) {
        return Err(invalid());
    }
    Ok(n as i64)
}
fn optional_string(
    v: &CommandJson,
    key: &str,
    max: usize,
    trim: bool,
) -> Result<Option<(String, CommandJson)>, BusinessError> {
    v.property(key)?.map(|x| text(&x, 1, max, trim)).transpose()
}
#[derive(Clone, Debug)]
pub struct InitialCountCreate {
    pub context_key: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub count_date: String,
    pub reason: String,
    pub quantity_milli: i64,
    pub unit_cost_minor: Option<i64>,
    pub lots: Vec<InitialCountLot>,
    pub normalized: CommandJson,
}
/// Original one-line Zod shape and field order are reproduced for exact replay.
pub fn normalize_initial_count_create(raw: &str) -> Result<InitialCountCreate, BusinessError> {
    let root = CommandJson::parse(raw)?;
    strict(&root, &["expectedVersion", "contextKey", "input"])?;
    if number(&prop(&root, "expectedVersion")?)? != 0 {
        return Err(invalid());
    }
    let (context, context_json) = text(&prop(&root, "contextKey")?, 64, 64, false)?;
    if !context
        .bytes()
        .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(invalid());
    }
    let input = prop(&root, "input")?;
    strict(&input, &["warehouseId", "countDate", "reason", "lines"])?;
    let (warehouse, wj) = text(&prop(&input, "warehouseId")?, 1, usize::MAX, false)?;
    let (day, dj) = text(&prop(&input, "countDate")?, 10, 10, false)?;
    if !date(&day) {
        return Err(invalid());
    }
    let (reason, rj) = text(&prop(&input, "reason")?, 5, 2000, true)?;
    let lines = prop(&input, "lines")?
        .array_elements()?
        .ok_or_else(invalid)?;
    if lines.len() != 1 {
        return Err(invalid());
    }
    let line = &lines[0];
    strict(
        line,
        &["productId", "countedQuantity", "unitCost", "allocations"],
    )?;
    let (product, pj) = text(&prop(line, "productId")?, 1, usize::MAX, false)?;
    let (qty, qj) = decimal(&prop(line, "countedQuantity")?, 3)?;
    let cost = line
        .property("unitCost")?
        .map(|v| decimal(&v, 2))
        .transpose()?;
    let allocations = match line.property("allocations")? {
        None => vec![],
        Some(x) => x.array_elements()?.ok_or_else(invalid)?,
    };
    if allocations.len() > 500 {
        return Err(invalid());
    }
    let mut lots = Vec::new();
    let mut encoded = Vec::new();
    for allocation in allocations {
        strict(
            &allocation,
            &[
                "lotId",
                "lotCode",
                "serialNumber",
                "manufacturedDate",
                "expiryDate",
                "countedQuantity",
                "unitCost",
            ],
        )?;
        let lot_id = optional_string(&allocation, "lotId", usize::MAX, false)?;
        let lot_code = optional_string(&allocation, "lotCode", 100, true)?;
        let serial = optional_string(&allocation, "serialNumber", 160, true)?;
        let day_value = |field: &str| -> Result<(Option<String>, CommandJson), BusinessError> {
            match allocation.property(field)? {
                None => Ok((None, CommandJson::parse("\"\"")?)),
                Some(v) => {
                    let (value, token) = text(&v, 0, 10, false)?;
                    if !value.is_empty() && !date(&value) {
                        return Err(invalid());
                    }
                    Ok(((!value.is_empty()).then_some(value), token))
                }
            }
        };
        let (manufactured, mj) = day_value("manufacturedDate")?;
        let (expiry, ej) = day_value("expiryDate")?;
        let (lqty, lqj) = decimal(&prop(&allocation, "countedQuantity")?, 3)?;
        let lcost = allocation
            .property("unitCost")?
            .map(|x| decimal(&x, 2))
            .transpose()?;
        let mut fields = Vec::new();
        if let Some((_, j)) = &lot_id {
            fields.push(format!("\"lotId\":{}", j.as_str()));
        }
        if let Some((_, j)) = &lot_code {
            fields.push(format!("\"lotCode\":{}", j.as_str()));
        }
        if let Some((_, j)) = &serial {
            fields.push(format!("\"serialNumber\":{}", j.as_str()));
        }
        fields.push(format!("\"manufacturedDate\":{}", mj.as_str()));
        fields.push(format!("\"expiryDate\":{}", ej.as_str()));
        fields.push(format!("\"countedQuantity\":{}", lqj.as_str()));
        if let Some((_, j)) = &lcost {
            fields.push(format!("\"unitCost\":{}", j.as_str()));
        }
        encoded.push(format!("{{{}}}", fields.join(",")));
        if lot_id.is_some() {
            return Err(BusinessError::invalid(
                "COUNT_LOT_INVALID",
                "Choose a current lot or serial from this warehouse.",
            ));
        }
        lots.push(InitialCountLot {
            lot_code: lot_code.map(|x| x.0),
            serial_number: serial.map(|x| x.0),
            manufactured_date: manufactured,
            expiry_date: expiry,
            quantity_milli: lqty,
            unit_cost_minor: lcost.as_ref().map_or(0, |x| x.0),
        });
    }
    let unit_cost = cost.as_ref().map(|(v, _)| *v);
    let cost_field = cost.as_ref().map_or(String::new(), |(_, j)| {
        format!(",\"unitCost\":{}", j.as_str())
    });
    let normalized = CommandJson::parse(&format!(
        "{{\"expectedVersion\":0,\"contextKey\":{},\"input\":{{\"warehouseId\":{},\"countDate\":{},\"reason\":{},\"lines\":[{{\"productId\":{},\"countedQuantity\":{}{cost_field},\"allocations\":[{}]}}]}}}}",
        context_json.as_str(),
        wj.as_str(),
        dj.as_str(),
        rj.as_str(),
        pj.as_str(),
        qj.as_str(),
        encoded.join(",")
    ))?;
    Ok(InitialCountCreate {
        context_key: context,
        warehouse_id: warehouse,
        product_id: product,
        count_date: day,
        reason,
        quantity_milli: qty,
        unit_cost_minor: unit_cost,
        lots,
        normalized,
    })
}
#[derive(Clone, Debug)]
pub struct InitialCountActionInput {
    pub expected_version: i64,
    pub attachment_set_ids: Vec<String>,
    pub reason: Option<String>,
    pub normalized: CommandJson,
}
pub fn normalize_initial_count_action(raw: &str) -> Result<InitialCountActionInput, BusinessError> {
    let input = CommandJson::parse(raw)?;
    strict(&input, &["expectedVersion", "attachmentSetIds", "reason"])?;
    let version = number(&prop(&input, "expectedVersion")?)?;
    if version == 0 {
        return Err(invalid());
    }
    let array = match input.property("attachmentSetIds")? {
        None => vec![],
        Some(v) => v.array_elements()?.ok_or_else(invalid)?,
    };
    if array.len() > 10 {
        return Err(invalid());
    }
    let mut ids = Vec::new();
    let mut tokens = Vec::new();
    for v in array {
        let (id, token) = text(&v, 1, usize::MAX, false)?;
        ids.push(id);
        tokens.push(token.as_str().to_owned());
    }
    let reason = input
        .property("reason")?
        .map(|v| text(&v, 3, 2000, true))
        .transpose()?;
    let reason_field = reason.as_ref().map_or(String::new(), |(_, v)| {
        format!(",\"reason\":{}", v.as_str())
    });
    let normalized = CommandJson::parse(&format!(
        "{{\"expectedVersion\":{version},\"attachmentSetIds\":[{}]{reason_field}}}",
        tokens.join(",")
    ))?;
    Ok(InitialCountActionInput {
        expected_version: version,
        attachment_set_ids: ids,
        reason: reason.map(|x| x.0),
        normalized,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Expected output is from the retained Zod schema under the pinned Bun.
    #[test]
    fn create_fingerprint_matches_retained_zod_field_order_and_defaults() {
        let raw = format!(
            r#"{{"expectedVersion":0,"contextKey":"{}","input":{{"warehouseId":"w","countDate":"2024-02-29","reason":"\ufeff First count \ufeff","lines":[{{"productId":"p","countedQuantity":"12.340","unitCost":"3.50","allocations":[{{"lotCode":"\ufeff Lot A \ufeff","countedQuantity":"12.340","unitCost":"3.50"}}]}}]}}}}"#,
            "a".repeat(64)
        );
        let normalized = normalize_initial_count_create(&raw).unwrap();
        assert_eq!(
            normalized.normalized.as_str(),
            format!(
                r#"{{"expectedVersion":0,"contextKey":"{}","input":{{"warehouseId":"w","countDate":"2024-02-29","reason":"First count","lines":[{{"productId":"p","countedQuantity":"12.340","unitCost":"3.50","allocations":[{{"lotCode":"Lot A","manufacturedDate":"","expiryDate":"","countedQuantity":"12.340","unitCost":"3.50"}}]}}]}}}}"#,
                "a".repeat(64)
            )
        );
    }

    #[test]
    fn action_fingerprint_matches_retained_zod_defaults() {
        let normalized = normalize_initial_count_action(
            r#"{"reason":"\ufeff review \ufeff","expectedVersion":3}"#,
        )
        .unwrap();
        assert_eq!(
            normalized.normalized.as_str(),
            r#"{"expectedVersion":3,"attachmentSetIds":[],"reason":"review"}"#
        );
    }
}
