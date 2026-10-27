//! Query only facts used by this exact source/ancestry and its candidate books.
//! Broad utility locks remain in image.rs, including all absence predicates.
use super::image::{ImageBudget, rows, text};
use super::*;
use std::collections::BTreeSet;

pub(super) struct Sources<'a> {
    pub ids: &'a [String],
    pub command: &'a JournalReviewCommand,
    pub chain: &'a [Value],
    pub journals: &'a [Value],
    pub lines: &'a [Value],
    pub money: &'a [Value],
    pub books: &'a [Value],
}
const FILTERS: &str = "WITH filters AS(SELECT split_part(value,':',1) kind,substr(value,strpos(value,':')+1) value FROM unnest($3::text[]) input(value))";
fn add(filters: &mut BTreeSet<String>, kind: &str, value: &str) {
    filters.insert(format!("{kind}:{value}"));
}
async fn query(
    c: &mut PgConnection,
    company: &CompanyId,
    source: &Sources<'_>,
    filters: &BTreeSet<String>,
    sql: &str,
    budget: &mut ImageBudget,
) -> Result<Vec<Value>, Error> {
    rows(
        c,
        &format!("{FILTERS} {sql}"),
        company,
        source.ids,
        &filters.iter().cloned().collect::<Vec<_>>(),
        budget,
    )
    .await
}
pub(super) async fn configurations(
    c: &mut PgConnection,
    company: &CompanyId,
    source: Sources<'_>,
    budget: &mut ImageBudget,
) -> Result<Vec<(String, Vec<Value>)>, Error> {
    let ancestors: BTreeSet<_> = source
        .chain
        .iter()
        .map(|r| text(r, "id"))
        .collect::<Result<_, _>>()?;
    let mut filters = BTreeSet::new();
    for header in source
        .journals
        .iter()
        .filter(|h| h["id"].as_str().is_some_and(|id| ancestors.contains(id)))
    {
        let chart = text(header, "chart_id")?;
        add(&mut filters, "primary-chart", chart);
        add(&mut filters, "chart", chart);
        add(&mut filters, "period", text(header, "period_id")?);
        add(&mut filters, "date", text(header, "entry_date")?);
        add(&mut filters, "currency", text(header, "currency")?);
    }
    if let JournalReviewInput::Reverse {
        period_id,
        entry_date,
        ..
    } = &source.command.input
    {
        add(&mut filters, "period", period_id);
        add(&mut filters, "date", entry_date);
    }
    for line in source.lines.iter().filter(|l| {
        l["journal_id"]
            .as_str()
            .is_some_and(|id| ancestors.contains(id))
    }) {
        let account = text(line, "account_id")?;
        add(&mut filters, "source-account", account);
        add(&mut filters, "account", account);
    }
    for row in source.money.iter().filter(|m| {
        m["source_id"]
            .as_str()
            .is_some_and(|id| ancestors.contains(id))
    }) {
        if let Some(id) = row["fx_rate_id"].as_str() {
            add(&mut filters, "rate", id);
        }
    }
    for row in source.books.iter().filter(|b| {
        b["primary_journal_id"]
            .as_str()
            .is_some_and(|id| ancestors.contains(id))
    }) {
        add(&mut filters, "retained-rule", text(row, "rule_id")?);
    }
    let mut output = Vec::new();
    output.push(("periods".into(),query(c,company,&source,&filters,"SELECT to_jsonb(p)::text FROM finance_periods p WHERE p.company_id=$1 AND (p.id IN(SELECT value FROM filters WHERE kind='period') OR EXISTS(SELECT 1 FROM filters d WHERE d.kind='date' AND d.value BETWEEN p.start_date AND p.end_date)) ORDER BY p.id LIMIT 20001",budget).await?));
    // Earliest starts and secondary ownership are predicate facts, not reasons
    // to load arbitrary historical rule bodies. Effective/actually retained
    // rules below keep their complete immutable row images.
    output.push(("bookStarts".into(),query(c,company,&source,&filters,"SELECT jsonb_build_object('primary_chart_id',r.primary_chart_id,'secondary_chart_id',r.secondary_chart_id,'first_date',min(r.effective_date))::text FROM finance_book_rules r WHERE r.company_id=$1 AND r.status='approved' AND r.primary_chart_id IN(SELECT value FROM filters WHERE kind='primary-chart') GROUP BY r.primary_chart_id,r.secondary_chart_id ORDER BY r.primary_chart_id,r.secondary_chart_id LIMIT 20001",budget).await?));
    output.push(("secondaryOwners".into(),query(c,company,&source,&filters,"SELECT jsonb_build_object('secondary_chart_id',r.secondary_chart_id)::text FROM finance_book_rules r WHERE r.company_id=$1 AND r.status='approved' AND r.secondary_chart_id IN(SELECT value FROM filters WHERE kind='primary-chart') GROUP BY r.secondary_chart_id ORDER BY r.secondary_chart_id LIMIT 20001",budget).await?));
    let rules=query(c,company,&source,&filters,"SELECT to_jsonb(r)::text FROM finance_book_rules r WHERE r.company_id=$1 AND (r.id IN(SELECT value FROM filters WHERE kind='retained-rule') OR (r.status='approved' AND r.primary_chart_id IN(SELECT value FROM filters WHERE kind='primary-chart') AND EXISTS(SELECT 1 FROM filters d WHERE d.kind='date' AND r.effective_date=(SELECT max(candidate.effective_date) FROM finance_book_rules candidate WHERE candidate.company_id=$1 AND candidate.status='approved' AND candidate.primary_chart_id=r.primary_chart_id AND candidate.secondary_chart_id=r.secondary_chart_id AND candidate.effective_date<=d.value)))) ORDER BY r.id LIMIT 20001",budget).await?;
    for rule in &rules {
        add(&mut filters, "rule", text(rule, "id")?);
        add(&mut filters, "chart", text(rule, "primary_chart_id")?);
        add(&mut filters, "chart", text(rule, "secondary_chart_id")?);
    }
    output.push(("rules".into(), rules));
    let mappings=query(c,company,&source,&filters,"SELECT to_jsonb(m)::text FROM finance_book_account_mappings m JOIN finance_book_rules r ON r.id=m.rule_id WHERE r.company_id=$1 AND m.rule_id IN(SELECT value FROM filters WHERE kind='rule') AND m.primary_account_id IN(SELECT value FROM filters WHERE kind='source-account') ORDER BY m.rule_id,m.primary_account_id LIMIT 20001",budget).await?;
    for mapping in &mappings {
        add(
            &mut filters,
            "account",
            text(mapping, "primary_account_id")?,
        );
        add(
            &mut filters,
            "account",
            text(mapping, "secondary_account_id")?,
        );
    }
    output.push(("mappings".into(), mappings));
    output.push(("charts".into(),query(c,company,&source,&filters,"SELECT to_jsonb(p)::text FROM finance_charts p WHERE p.company_id=$1 AND p.id IN(SELECT value FROM filters WHERE kind='chart') ORDER BY p.id LIMIT 20001",budget).await?));
    output.push(("accounts".into(),query(c,company,&source,&filters,"SELECT to_jsonb(p)::text FROM finance_accounts p WHERE p.company_id=$1 AND p.id IN(SELECT value FROM filters WHERE kind='account') ORDER BY p.id LIMIT 20001",budget).await?));
    let dimensions=query(c,company,&source,&filters,"SELECT to_jsonb(d)::text FROM finance_dimension_definitions d WHERE d.company_id=$1 AND ((d.status='active' AND d.required=1) OR EXISTS(SELECT 1 FROM finance_journal_lines l WHERE l.journal_id=ANY($2) AND jsonb_typeof(l.dimension_json::jsonb)='object' AND l.dimension_json::jsonb ? d.code)) ORDER BY d.id LIMIT 20001",budget).await?;
    for dimension in &dimensions {
        add(&mut filters, "dimension", text(dimension, "id")?);
    }
    output.push(("dimensions".into(), dimensions));
    output.push(("dimensionValues".into(),query(c,company,&source,&filters,"SELECT to_jsonb(v)::text FROM finance_dimension_values v JOIN finance_dimension_definitions d ON d.id=v.dimension_id AND d.company_id=v.company_id WHERE v.company_id=$1 AND d.id IN(SELECT value FROM filters WHERE kind='dimension') AND EXISTS(SELECT 1 FROM finance_journal_lines l WHERE l.journal_id=ANY($2) AND jsonb_typeof(l.dimension_json::jsonb)='object' AND l.dimension_json::jsonb->>d.code=v.code) ORDER BY v.id LIMIT 20001",budget).await?));
    // Keep every ID tie in the selected date/confirmation group so the proven
    // native en-US collator, rather than PostgreSQL collation, breaks that tie.
    output.push(("rates".into(),query(c,company,&source,&filters,"SELECT (to_jsonb(r)||jsonb_build_object('currency_per_usd',r.currency_per_usd::text))::text FROM company_fx_rates r WHERE r.company_id=$1 AND (r.id IN(SELECT value FROM filters WHERE kind='rate') OR EXISTS(SELECT 1 FROM filters currency CROSS JOIN filters day WHERE currency.kind='currency' AND day.kind='date' AND r.currency=currency.value AND ((r.effective_date::text=(SELECT max(p.effective_date)::text FROM company_fx_rates p WHERE p.company_id=$1 AND p.currency=r.currency AND p.effective_date::text<=day.value) AND r.confirmed_at=(SELECT max(p.confirmed_at) FROM company_fx_rates p WHERE p.company_id=$1 AND p.currency=r.currency AND p.effective_date=r.effective_date)) OR (NOT EXISTS(SELECT 1 FROM company_fx_rates p WHERE p.company_id=$1 AND p.currency=r.currency AND p.effective_date::text<=day.value) AND r.confirmed_at=(SELECT max(p.confirmed_at) FROM company_fx_rates p WHERE p.company_id=$1 AND p.currency=r.currency))))) ORDER BY r.id LIMIT 20001",budget).await?));
    output.push(("scales".into(),query(c,company,&source,&filters,"SELECT to_jsonb(p)::text FROM company_currency_scales p WHERE p.company_id=$1 AND p.currency IN(SELECT value FROM filters WHERE kind='currency') ORDER BY p.currency LIMIT 20001",budget).await?));
    Ok(output)
}
