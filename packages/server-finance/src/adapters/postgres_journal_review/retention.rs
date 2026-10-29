use super::image::{configuration, integer, text};
use super::*;
use serde_json::json;
use std::collections::BTreeMap;

fn rows_equal(a: &[Value], b: &[Value]) -> bool {
    fn key(row: &Value) -> Option<(&str, &str, &str)> {
        if let Some(id) = row["id"].as_str() {
            Some(("id", id, ""))
        } else if let (Some(id), Some(kind)) =
            (row["source_id"].as_str(), row["amount_kind"].as_str())
        {
            Some(("money", id, kind))
        } else if let (Some(primary), Some(rule)) =
            (row["primary_journal_id"].as_str(), row["rule_id"].as_str())
        {
            Some(("book", primary, rule))
        } else {
            None
        }
    }
    fn index(rows: &[Value]) -> Option<BTreeMap<(&str, &str, &str), &Value>> {
        let mut index = BTreeMap::new();
        for row in rows {
            if index.insert(key(row)?, row).is_some() {
                return None;
            }
        }
        Some(index)
    }
    // Borrowed stable keys and full Value equality avoid serializing every row
    // again merely to sort it, and reject duplicate identities explicitly.
    match (index(a), index(b)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}
fn by_id<'a>(rows: &'a [Value], id: &str) -> Result<&'a Value, Error> {
    rows.iter().find(|v| v["id"] == id).ok_or_else(changed)
}
fn replace(rows: &mut [Value], id: &str, value: Value) -> Result<(), Error> {
    let row = rows
        .iter_mut()
        .find(|r| r["id"] == id)
        .ok_or_else(changed)?;
    *row = value;
    Ok(())
}
fn primary_headers(
    source: &PgJournalReviewSource,
    plan: &JournalReviewPlan,
) -> Result<Vec<Value>, Error> {
    let mut rows: Vec<_> = source
        .before
        .journals
        .iter()
        .filter(|j| source.primary_ids.iter().any(|id| j["id"] == *id))
        .cloned()
        .collect();
    let mut original = by_id(&rows, &source.journal.id)?.clone();
    let user = Value::from(source.actor.user_id.as_str());
    let now = Value::from(plan.now().get());
    original["version"] = (integer(&original, "version")? + 1).into();
    original["updated_at"] = now.clone();
    match plan.action() {
        JournalReviewAction::Post { .. } => {
            original["status"] = "posted".into();
            original["posted_by"] = user;
            original["posted_at"] = now;
            replace(&mut rows, &source.journal.id, original)?;
        }
        JournalReviewAction::Reverse {
            period_id,
            entry_date,
            memo,
            reversal_id,
            number,
            ..
        } => {
            if rows.iter().any(|j| j["id"] == *reversal_id) {
                return Err(changed());
            }
            original["status"] = "reversed".into();
            original["reversed_by"] = user.clone();
            original["reversed_at"] = now.clone();
            replace(&mut rows, &source.journal.id, original.clone())?;
            let mut created = original;
            created["id"] = reversal_id.clone().into();
            created["period_id"] = period_id.clone().into();
            created["journal_number"] = number.clone().into();
            created["entry_date"] = entry_date.clone().into();
            created["source_type"] = "reversal".into();
            created["source_id"] = source.journal.id.clone().into();
            created["memo"] = memo.clone().into();
            created["status"] = "posted".into();
            created["version"] = 1.into();
            created["is_opening"] = 0.into();
            created["created_by"] = user.clone();
            created["posted_by"] = user;
            created["posted_at"] = now.clone();
            created["created_at"] = now.clone();
            created["updated_at"] = now;
            created["reversed_by"] = Value::Null;
            created["reversed_at"] = Value::Null;
            rows.push(created);
        }
    }
    Ok(rows)
}
fn primary_lines(
    source: &PgJournalReviewSource,
    plan: &JournalReviewPlan,
) -> Result<Vec<Value>, Error> {
    let mut rows: Vec<_> = source
        .before
        .lines
        .iter()
        .filter(|l| source.primary_ids.iter().any(|id| l["journal_id"] == *id))
        .cloned()
        .collect();
    if let JournalReviewAction::Reverse { reversal_id, .. } = plan.action() {
        let created = rows
            .iter()
            .filter(|l| l["journal_id"] == source.journal.id)
            .map(|line| {
                let mut l = line.clone();
                l["id"] = format!("{reversal_id}-{}", integer(line, "ordinal")?).into();
                l["journal_id"] = reversal_id.clone().into();
                l["debit_minor"] = line["credit_minor"].clone();
                l["credit_minor"] = line["debit_minor"].clone();
                l["base_debit_minor"] = line["base_credit_minor"].clone();
                l["base_credit_minor"] = line["base_debit_minor"].clone();
                Ok(l)
            })
            .collect::<Result<Vec<_>, Error>>()?;
        rows.extend(created);
    }
    Ok(rows)
}
pub(super) fn primary(
    source: &PgJournalReviewSource,
    plan: &JournalReviewPlan,
    money: &Value,
    after: &Image,
) -> Result<(), Error> {
    if source.before.chain != after.chain
        || source.before.configurations != after.configurations
        || source.before.allocations != after.allocations
    {
        return Err(changed());
    }
    let actual_headers: Vec<_> = after
        .journals
        .iter()
        .filter(|j| source.primary_ids.iter().any(|id| j["id"] == *id))
        .cloned()
        .collect();
    if !rows_equal(&primary_headers(source, plan)?, &actual_headers) {
        return Err(changed());
    }
    let actual_lines: Vec<_> = after
        .lines
        .iter()
        .filter(|l| source.primary_ids.iter().any(|id| l["journal_id"] == *id))
        .cloned()
        .collect();
    if !rows_equal(&primary_lines(source, plan)?, &actual_lines) {
        return Err(changed());
    }
    let mut expected_money = source.before.money.clone();
    if expected_money
        .iter()
        .any(|m| m["source_id"] == money["source_id"] && m["amount_kind"] == money["amount_kind"])
    {
        return Err(changed());
    }
    expected_money.push(money.clone());
    if !rows_equal(&expected_money, &after.money) {
        return Err(changed());
    }
    let mut invoices = source.before.invoices.clone();
    if plan.opening() {
        for invoice in invoices
            .iter_mut()
            .filter(|i| i["source_id"] == source.journal.id)
        {
            invoice["version"] = (integer(invoice, "version")? + 1).into();
            invoice["updated_at"] = plan.now().get().into();
            match plan.action() {
                JournalReviewAction::Post { .. } => {
                    invoice["status"] = "posted".into();
                    invoice["approved_by"] = source.actor.user_id.as_str().into();
                    invoice["posted_at"] = plan.now().get().into();
                }
                JournalReviewAction::Reverse { .. } => {
                    invoice["status"] = "voided".into();
                    invoice["outstanding_minor"] = 0.into();
                    invoice["base_outstanding_minor"] = 0.into();
                }
            }
        }
    }
    if !rows_equal(&invoices, &after.invoices) {
        return Err(changed());
    }
    for event in &source.before.events {
        if by_id(&after.events, text(event, "id")?)? != event {
            return Err(changed());
        }
    }
    Ok(())
}
fn bounded_database_time(
    source: &PgJournalReviewSource,
    row: &Value,
    key: &str,
    now: i64,
) -> Result<Value, Error> {
    let at = integer(row, key)?;
    if at < source.database_at || at > now {
        return Err(changed());
    }
    Ok(at.into())
}
pub(super) fn complete(
    source: &PgJournalReviewSource,
    plan: &JournalReviewPlan,
    money: &Value,
    after: &Image,
    database_now: i64,
) -> Result<(), Error> {
    primary(source, plan, money, after)?;
    let mut headers = primary_headers(source, plan)?;
    let mut lines = primary_lines(source, plan)?;
    let mut books = source.before.books.clone();
    // Existing actual secondary books follow only their exact posted primary's
    // reversal. No source_type/source_id string manufactures a book link.
    for old in source
        .before
        .journals
        .iter()
        .filter(|j| !source.primary_ids.iter().any(|id| j["id"] == *id))
    {
        let mut child = old.clone();
        if matches!(plan.action(), JournalReviewAction::Reverse { .. })
            && child["status"] == "posted"
            && source.before.books.iter().any(|b| {
                b["primary_journal_id"] == source.journal.id
                    && b["secondary_journal_id"] == child["id"]
            })
        {
            child["status"] = "reversed".into();
            child["version"] = (integer(&child, "version")? + 1).into();
            child["reversed_by"] = source.actor.user_id.as_str().into();
            child["reversed_at"] = plan.now().get().into();
            child["updated_at"] = plan.now().get().into();
        }
        headers.push(child);
    }
    lines.extend(
        source
            .before
            .lines
            .iter()
            .filter(|l| !source.primary_ids.iter().any(|id| l["journal_id"] == *id))
            .cloned(),
    );
    let primary_id = match plan.action() {
        JournalReviewAction::Post { .. } => source.journal.id.as_str(),
        JournalReviewAction::Reverse { reversal_id, .. } => reversal_id.as_str(),
    };
    let posted = by_id(&headers, primary_id)?.clone();
    let mut new_ids = vec![primary_id.to_owned()];
    if !matches!(
        text(&posted, "source_type")?,
        "book_projection" | "book_opening" | "book_adjustment"
    ) {
        let mut effective = BTreeMap::<String, &Value>::new();
        for rule in configuration(&source.before, "rules")?
            .iter()
            .filter(|r| r["status"] == "approved" && r["primary_chart_id"] == posted["chart_id"])
        {
            if text(rule, "effective_date")? > text(&posted, "entry_date")? {
                continue;
            }
            let key = text(rule, "secondary_chart_id")?.to_owned();
            if effective
                .get(&key)
                .is_none_or(|old| old["effective_date"].as_str() < rule["effective_date"].as_str())
            {
                effective.insert(key, rule);
            }
        }
        for (secondary, rule) in effective {
            let rid = text(rule, "id")?;
            if books
                .iter()
                .any(|b| b["primary_journal_id"] == primary_id && b["rule_id"] == rid)
            {
                return Err(changed());
            }
            let child_id = format!("book-{rid}-{primary_id}");
            let mut child = posted.clone();
            child["id"] = child_id.clone().into();
            child["journal_number"] = child_id.clone().into();
            child["chart_id"] = secondary.into();
            child["source_type"] = "book_projection".into();
            child["source_id"] = primary_id.into();
            headers.push(child);
            let source_lines: Vec<_> = lines
                .iter()
                .filter(|l| l["journal_id"] == primary_id)
                .cloned()
                .collect();
            for original in source_lines {
                let map = configuration(&source.before, "mappings")?
                    .iter()
                    .find(|m| {
                        m["rule_id"] == rid && m["primary_account_id"] == original["account_id"]
                    })
                    .ok_or_else(changed)?;
                let mut line = original.clone();
                line["id"] = format!("{child_id}-{}", integer(&line, "ordinal")?).into();
                line["journal_id"] = child_id.clone().into();
                line["account_id"] = map["secondary_account_id"].clone();
                lines.push(line);
            }
            let actual = after
                .books
                .iter()
                .find(|b| b["primary_journal_id"] == primary_id && b["rule_id"] == rid)
                .ok_or_else(changed)?;
            books.push(json!({"primary_journal_id":primary_id,"secondary_journal_id":child_id,"rule_id":rid,"company_id":source.actor.company_id.as_str(),"source_version":posted["version"],"created_at":bounded_database_time(source,actual,"created_at",database_now)?}));
            new_ids.push(child_id);
        }
    }
    if !rows_equal(&headers, &after.journals)
        || !rows_equal(&lines, &after.lines)
        || !rows_equal(&books, &after.books)
    {
        return Err(changed());
    }
    let mut events = source.before.events.clone();
    for id in new_ids {
        if events.iter().any(|e| e["journal_id"] == id) {
            return Err(changed());
        }
        let header = by_id(&headers, &id)?;
        let actual = after
            .events
            .iter()
            .find(|e| e["journal_id"] == id)
            .ok_or_else(changed)?;
        let chart = configuration(&source.before, "charts")?
            .iter()
            .find(|c| c["id"] == header["chart_id"])
            .ok_or_else(changed)?;
        let mut journal_lines: Vec<_> = lines
            .iter()
            .filter(|l| l["journal_id"] == id)
            .cloned()
            .collect();
        journal_lines.sort_by_key(|l| l["ordinal"].as_i64());
        let source_type = text(header, "source_type")?;
        let event_kind = if source_type.contains("reversal") {
            "reversal"
        } else if matches!(source_type, "credit_note" | "debit_note") {
            "credit_debit_note"
        } else if source_type.contains("fx") {
            "fx_adjustment"
        } else if source_type.contains("payment") || source_type == "return_refund" {
            "settlement"
        } else {
            "recognition"
        };
        let expected = json!({"id":format!("journal-event-{id}"),"company_id":header["company_id"],"chart_id":header["chart_id"],"period_id":header["period_id"],"journal_id":id,"source_type":header["source_type"],"source_id":header["source_id"],"source_version":header["version"],"effective_date":header["entry_date"],"currency":header["currency"],"base_currency":chart["base_currency"],"event_kind":event_kind,"evidence_json":json!({"journal":header,"lines":journal_lines}),"legacy_snapshot":0,"recorded_at":bounded_database_time(source,actual,"recorded_at",database_now)?});
        let mut observed = actual.clone();
        observed["evidence_json"] =
            serde_json::from_str(text(actual, "evidence_json")?).map_err(|_| changed())?;
        if observed != expected {
            return Err(changed());
        }
        events.push(actual.clone());
    }
    if !rows_equal(&events, &after.events) {
        return Err(changed());
    }
    Ok(())
}
