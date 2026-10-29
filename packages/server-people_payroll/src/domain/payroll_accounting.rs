//! Inactive, non-authoritative source-shaped Payroll accounting projection.
use super::{
    payroll_accounting_collation, payroll_accounting_date as date,
    payroll_accounting_operand::{Fault, Operand, R, Text, bigint, js_print, js_round},
};
use num_bigint::BigInt;
use num_traits::{ToPrimitive, Zero};
use serde::Serialize;

#[derive(Clone, Debug)]
pub struct PayrollProjectionItem(pub(crate) Operand);
#[derive(Clone, Debug)]
pub struct PayrollProjectionAccounts(pub(crate) Operand);
#[derive(Debug, Serialize)]
pub struct PayrollSourceFailure {
    pub name: &'static str,
    pub status: Option<u16>,
    pub code: Option<&'static str>,
    pub message: String,
}
pub enum PayrollProjectionOutcome<T> {
    Value(T),
    SourceFailure(PayrollSourceFailure),
    CompatibilityGap {
        gate: &'static str,
        operation: &'static str,
        field_path: String,
    },
}
impl<T> From<R<T>> for PayrollProjectionOutcome<T> {
    fn from(v: R<T>) -> Self {
        match v {
            Ok(x) => Self::Value(x),
            Err(Fault::Source {
                name,
                status,
                code,
                message,
            }) => Self::SourceFailure(PayrollSourceFailure {
                name,
                status,
                code,
                message,
            }),
            Err(Fault::Gap {
                gate,
                operation,
                field_path,
            }) => Self::CompatibilityGap {
                gate,
                operation,
                field_path,
            },
        }
    }
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PayrollDimensions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<Text>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub department_id: Option<Text>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PayrollCostShare {
    pub amount_minor: f64,
    pub dimensions: PayrollDimensions,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PayrollJournalLine {
    pub account_id: Text,
    pub description: String,
    pub debit: String,
    pub credit: String,
    pub dimensions: PayrollDimensions,
}
#[derive(Default, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PayrollTotals {
    pub gross_minor: f64,
    pub deductions_minor: f64,
    pub employer_minor: f64,
    pub net_minor: f64,
    pub debit_minor: f64,
    pub credit_minor: f64,
}
#[derive(Debug)]
pub struct Categories(Vec<(String, f64)>);
impl Serialize for Categories {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut m = s.serialize_map(Some(self.0.len()))?;
        for (k, v) in &self.0 {
            m.serialize_entry(k, v)?
        }
        m.end()
    }
}
#[derive(Debug, Serialize)]
pub struct PayrollAccountingProjection {
    pub lines: Vec<PayrollJournalLine>,
    pub totals: PayrollTotals,
    pub categories: Categories,
}
fn fail(code: &'static str, message: &'static str) -> Fault {
    Fault::business(409, code, message)
}
fn missing_components(posting: bool) -> Fault {
    fail(
        "PAYROLL_COMPONENTS_MISSING",
        if posting {
            "Recalculate payroll to retain the item components before posting."
        } else {
            "Recalculate payroll to retain item components before settlement."
        },
    )
}
fn parse_document(item: &Operand, key: &str) -> R<Operand> {
    let value = item.prop(key)?;
    let text = value
        .string()
        .ok_or_else(|| Fault::gap("P1", "JSON.parse", key))?;
    let source = text.utf8()?;
    Operand::raw(&source)
}
fn get_array(v: &Operand, key: &str) -> R<Vec<Operand>> {
    let value = v.prop(key)?;
    if value.nullish() {
        return Ok(Vec::new());
    }
    value
        .array()?
        .ok_or_else(|| Fault::gap("P1", "iterate", key))
}
fn components(result: &Operand, posting: bool) -> R<Vec<Operand>> {
    let exp = result.prop("explanation")?;
    if exp.nullish() {
        return Err(missing_components(posting));
    }
    exp.prop("components")?
        .array()?
        .ok_or_else(|| missing_components(posting))
}
fn optional(v: &Operand, key: &str) -> R<Operand> {
    if v.nullish() {
        Ok(Operand::Missing)
    } else {
        v.prop(key)
    }
}
fn selected(v: &Operand, key: &str) -> R<Option<Text>> {
    let v = v.prop(key)?;
    if v.nullish() {
        Ok(None)
    } else {
        v.string()
            .map(Some)
            .ok_or_else(|| Fault::gap("P1", "identity", key))
    }
}
fn eq_optional(a: &Operand, b: &Operand) -> R<bool> {
    let tag = |v: &Operand| match v.token() {
        None => 0,
        Some("null") => 1,
        Some("true" | "false") => 2,
        Some(s) if s.starts_with('"') => 3,
        Some(s) if s.starts_with('[') || s.starts_with('{') => 5,
        Some(_) => 4,
    };
    let left = tag(a);
    let right = tag(b);
    if left == 5 || right == 5 {
        return Err(Fault::gap("P1", "strict equality", "object identity"));
    }
    if left != right {
        return Ok(false);
    }
    match left {
        0 | 1 => Ok(true),
        2 => Ok(a.token() == b.token()),
        3 => Ok(a.string() == b.string()),
        4 => Ok(a.number()? == b.number()?),
        _ => unreachable!(),
    }
}
fn amount_component(c: &Operand) -> R<f64> {
    c.prop("amountMinor")?.number()
}
fn allocate_component(c: &Operand, weights: &[BigInt]) -> R<Vec<f64>> {
    let amount = c.prop("amountMinor")?;
    if !amount.actual_safe_integer() {
        return Err(fail(
            "PAYROLL_COST_ALLOCATION_INVALID",
            "Cost allocation amount is invalid.",
        ));
    }
    allocate(amount.number()?, weights)
}
fn kind(c: &Operand, name: &str) -> R<bool> {
    Ok(c.prop("kind")?.strict_text(name))
}
fn safe(n: f64) -> bool {
    n.is_finite() && n.fract() == 0.0 && n.abs() <= 9007199254740991.0
}
fn source_number_sum(values: &[Operand]) -> R<f64> {
    let mut number = 0.0;
    let mut text: Option<String> = None;
    for v in values {
        if let Some(t) = v.string() {
            let s = t.utf8()?;
            if let Some(x) = &mut text {
                x.push_str(&s)
            } else {
                text = Some(js_print(number) + &s)
            }
        } else {
            let n = v.number()?;
            if let Some(x) = &mut text {
                x.push_str(&js_print(n))
            } else {
                number += n
            }
        }
    }
    if let Some(s) = text {
        Operand::text(&s).number()
    } else {
        Ok(number)
    }
}
fn category(c: &Operand, k: &str) -> R<String> {
    let code = c.prop("code")?;
    let Some(text) = code.string() else {
        return Err(Fault::native(
            "TypeError",
            if code.token().is_none() {
                "Cannot read properties of undefined (reading 'toUpperCase')"
            } else if code.token() == Some("null") {
                "Cannot read properties of null (reading 'toUpperCase')"
            } else {
                "code.toUpperCase is not a function"
            },
        ));
    };
    let n = text.upper_key();
    let has = |t: &str| n.contains(t);
    let token = |t: &str| n.split('_').any(|x| x == t);
    let out = if k == "earning" {
        if has("SERVICE_CHARGE") {
            "service_charge"
        } else if has("BONUS") || has("INCENTIVE") {
            "bonus"
        } else if has("OVERTIME") || token("OT") {
            "overtime"
        } else {
            "wages"
        }
    } else if n.starts_with("PAYROLL_ADVANCE_") {
        "advance"
    } else if n.starts_with("BENEFIT_") {
        "benefit"
    } else if has("SOCIAL") || has("SSO") {
        "social_security"
    } else if has("TAX") || has("WITHHOLD") || token("PIT") || has("PND1") {
        "tax"
    } else {
        let original = text.utf8()?;
        if original.to_lowercase().starts_with("thai:") {
            return Err(Fault::business(
                422,
                "PAYROLL_COMPONENT_UNMAPPED",
                format!("No accounting classification exists for statutory component {original}."),
            ));
        }
        if k == "employer" {
            "employer_other"
        } else {
            "deduction_other"
        }
    };
    Ok(out.into())
}
#[derive(Clone)]
struct Unit {
    branch: Option<Text>,
    department: Option<Text>,
    days: BigInt,
    regular: BigInt,
    overtime: BigInt,
}
impl Unit {
    fn dims(&self) -> PayrollDimensions {
        PayrollDimensions {
            branch_id: self.branch.clone().filter(|x| !x.0.is_empty()),
            department_id: self.department.clone().filter(|x| !x.0.is_empty()),
        }
    }
    fn key(&self) -> String {
        format!(
            "[{},{}]",
            self.branch.as_ref().map_or("null".into(), Text::json),
            self.department.as_ref().map_or("null".into(), Text::json)
        )
    }
}
fn as_text(v: &Operand, path: &str) -> R<Text> {
    v.string().ok_or_else(|| Fault::gap("P1", "text", path))
}
fn unit_for(units: &mut Vec<Unit>, a: &Operand) -> R<usize> {
    let branch = selected(a, "branch_id")?;
    let department = selected(a, "department_id")?;
    if let Some(i) = units
        .iter()
        .position(|x| x.branch == branch && x.department == department)
    {
        return Ok(i);
    }
    units.push(Unit {
        branch,
        department,
        days: BigInt::zero(),
        regular: BigInt::zero(),
        overtime: BigInt::zero(),
    });
    Ok(units.len() - 1)
}
fn at(assignments: &[Operand], day: &Text) -> R<Operand> {
    let mut found = Vec::new();
    for a in assignments {
        let from = as_text(&a.prop("effective_from")?, "effective_from")?;
        let to = a.prop("effective_to")?;
        if from.0 <= day.0
            && (to.token() == Some("null") || as_text(&to, "effective_to")?.0 >= day.0)
        {
            found.push(a.clone())
        }
    }
    if found.len() != 1 || !found[0].prop("evidence_status")?.strict_text("verified") {
        return Err(fail(
            "PAYROLL_COST_ALLOCATION_INCOMPLETE",
            "Dated branch and department evidence is incomplete or ambiguous.",
        ));
    }
    Ok(found.remove(0))
}
fn cost_units(snapshot: &Operand) -> R<Vec<Unit>> {
    let period = snapshot.prop("period")?;
    let engagement = snapshot.prop("engagement")?;
    let start = optional(&engagement, "start_date")?;
    let assignments = snapshot.prop("costAssignments")?.array()?;
    if period.nullish() || !start.truthy() || assignments.as_ref().is_none_or(Vec::is_empty) {
        return Err(fail(
            "PAYROLL_COST_ALLOCATION_INCOMPLETE",
            "Dated cost assignments are missing from the approved payroll snapshot.",
        ));
    }
    let assignments = assignments.unwrap();
    let ps = as_text(&period.prop("start")?, "period.start")?;
    let pe = as_text(&period.prop("end")?, "period.end")?;
    let es = as_text(&start, "engagement.start_date")?;
    let ee = optional(&engagement, "end_date")?;
    let start = if es.0 > ps.0 { es } else { ps };
    let end = if ee.truthy() {
        let e = as_text(&ee, "engagement.end_date")?;
        if e.0 < pe.0 { e } else { pe }
    } else {
        pe
    };
    let mut units = Vec::new();
    if let (Some(mut day), Some(last)) = (date::parse(&start)?, date::parse(&end)?) {
        while day <= last {
            let a = at(&assignments, &date::iso_day(day))?;
            let i = unit_for(&mut units, &a)?;
            units[i].days += 1;
            day += 86400000;
        }
    }
    let hours = snapshot.prop("hours")?;
    let entries = if hours.nullish() {
        Vec::new()
    } else {
        get_array(&hours, "entries")?
    };
    for e in entries {
        let day = as_text(&e.prop("work_date")?, "work_date")?;
        let a = at(&assignments, &day)?;
        for (l, r) in [
            ("cost_assignment_id", "id"),
            ("cost_branch_id", "branch_id"),
            ("cost_department_id", "department_id"),
        ] {
            if !eq_optional(&e.prop(l)?, &a.prop(r)?)? {
                return Err(fail(
                    "PAYROLL_TIME_COST_EVIDENCE_MISSING",
                    "Approved time does not match its dated cost assignment.",
                ));
            }
        }
        let i = unit_for(&mut units, &a)?;
        units[i].regular += bigint(js_round(e.prop("regular_hours")?.number()? * 1_000_000.0))?;
        units[i].overtime += bigint(js_round(e.prop("overtime_hours")?.number()? * 1_000_000.0))?;
    }
    let handoffs = if hours.nullish() {
        Vec::new()
    } else {
        get_array(&hours, "handoffs")?
    };
    for h in handoffs {
        let a = at(&assignments, &as_text(&h.prop("work_date")?, "work_date")?)?;
        let i = unit_for(&mut units, &a)?;
        units[i].overtime += bigint(js_round(h.prop("minutes")?.number()? * 1_000_000.0 / 60.0))?;
    }
    let collator = payroll_accounting_collation::collator()?;
    units.sort_by(|a, b| collator.compare(&a.key(), &b.key()));
    Ok(units)
}
pub(crate) fn allocate(amount: f64, weights: &[BigInt]) -> R<Vec<f64>> {
    if !safe(amount) || amount < 0.0 {
        return Err(fail(
            "PAYROLL_COST_ALLOCATION_INVALID",
            "Cost allocation amount is invalid.",
        ));
    }
    if amount == 0.0 {
        return Ok(vec![0.0; weights.len()]);
    }
    let total: BigInt = weights.iter().sum();
    if total <= BigInt::zero() {
        return Err(fail(
            "PAYROLL_COST_ALLOCATION_INCOMPLETE",
            "Approved dated work or placement evidence is missing for a payroll amount.",
        ));
    }
    let a = bigint(amount)?;
    let mut parts: Vec<(usize, f64, BigInt)> = weights
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let product = &a * w;
            (
                i,
                (&product / &total).to_f64().unwrap_or_else(|| {
                    if product < Zero::zero() {
                        f64::NEG_INFINITY
                    } else {
                        f64::INFINITY
                    }
                }),
                product % &total,
            )
        })
        .collect();
    let mut remaining = amount - parts.iter().map(|p| p.1).sum::<f64>();
    let mut order: Vec<usize> = (0..parts.len()).collect();
    order.sort_by(|a, b| parts[*b].2.cmp(&parts[*a].2).then(a.cmp(b)));
    for i in order {
        if remaining <= 0.0 {
            break;
        }
        parts[i].1 += 1.0;
        remaining -= 1.0
    }
    Ok(parts.into_iter().map(|p| p.1).collect())
}
struct Allocation {
    units: Vec<Unit>,
    wages: Vec<f64>,
    overtime: Vec<f64>,
    earnings: Vec<Vec<f64>>,
    deductions: Vec<Vec<f64>>,
    employers: Vec<Vec<f64>>,
    net: Vec<f64>,
}
fn allocate_item(item: &Operand, snapshot: &Operand, components: &[Operand]) -> R<Allocation> {
    let units = cost_units(snapshot)?;
    let result = parse_document(item, "result_json")?;
    let explanation = result.prop("explanation")?;
    let mut earnings = Vec::new();
    for c in components {
        if kind(c, "earning")? {
            earnings.push(c.clone())
        }
    }
    let amounts: Vec<Operand> = earnings
        .iter()
        .map(|c| c.prop("amountMinor"))
        .collect::<R<_>>()?;
    let base = item.prop("gross_minor")?.number()? - source_number_sum(&amounts)?;
    let policy = snapshot.prop("policy")?;
    let em = optional(&explanation, "overtimeMultiplierBps")?;
    let pm = optional(&policy, "overtimeMultiplierBps")?;
    let multiplier = if !em.nullish() {
        em.number()?
    } else if !pm.nullish() {
        pm.number()?
    } else {
        15000.0
    };
    let explicit = optional(&explanation, "overtimeMinor")?;
    let overtime = if !explicit.nullish() {
        explicit.number()?
    } else {
        let rate = optional(&explanation, "hourlyRateMinor")?;
        let hours = optional(&explanation, "overtimeHours")?;
        js_round(
            (if rate.nullish() { 0.0 } else { rate.number()? })
                * (if hours.nullish() {
                    0.0
                } else {
                    hours.number()?
                })
                * multiplier
                / 10000.0,
        )
    };
    let regular = base - overtime;
    let remainder = base - regular;
    if regular < 0.0 || remainder < 0.0 {
        return Err(fail(
            "PAYROLL_COST_ALLOCATION_INVALID",
            "Wage and overtime amounts do not reconcile.",
        ));
    }
    let mut time = Vec::new();
    for u in &units {
        time.push(&u.regular * BigInt::from(10000) + &u.overtime * bigint(multiplier)?)
    }
    let engagement = snapshot.prop("engagement")?;
    let salary = optional(&engagement, "employment_type")?.strict_text("salaried");
    let general: Vec<BigInt> = if salary {
        units.iter().map(|u| u.days.clone()).collect()
    } else {
        time
    };
    let regular_weights: Vec<BigInt> = units
        .iter()
        .map(|u| {
            if salary {
                u.days.clone()
            } else {
                u.regular.clone()
            }
        })
        .collect();
    let overtime_weights: Vec<BigInt> = units.iter().map(|u| u.overtime.clone()).collect();
    let wages = allocate(regular, &regular_weights)?;
    let overtime = allocate(remainder, &overtime_weights)?;
    let mut earn_parts = Vec::new();
    for c in &earnings {
        let weights = if category(c, "earning")? == "overtime" {
            &overtime_weights
        } else {
            &general
        };
        earn_parts.push(allocate_component(c, weights)?)
    }
    let gross: Vec<f64> = (0..units.len())
        .map(|i| wages[i] + overtime[i] + earn_parts.iter().map(|p| p[i]).sum::<f64>())
        .collect();
    let mut deductions = Vec::new();
    let mut employers = Vec::new();
    for c in components {
        if kind(c, "deduction")? {
            let weights: Vec<BigInt> = gross.iter().map(|n| bigint(*n)).collect::<R<_>>()?;
            deductions.push(allocate_component(c, &weights)?);
        }
    }
    for c in components {
        if kind(c, "employer")? {
            let weights: Vec<BigInt> = gross.iter().map(|n| bigint(*n)).collect::<R<_>>()?;
            employers.push(allocate_component(c, &weights)?);
        }
    }
    let net: Vec<f64> = (0..units.len())
        .map(|i| gross[i] - deductions.iter().map(|p| p[i]).sum::<f64>())
        .collect();
    if net.iter().any(|n| *n < 0.0)
        || net.iter().sum::<f64>() != item.prop("net_minor")?.number()?
    {
        return Err(fail(
            "PAYROLL_COST_ALLOCATION_INVALID",
            "Cost-unit payable amounts do not reconcile.",
        ));
    }
    Ok(Allocation {
        units,
        wages,
        overtime,
        earnings: earn_parts,
        deductions,
        employers,
        net,
    })
}
fn shares_inner(item: &PayrollProjectionItem) -> R<Vec<PayrollCostShare>> {
    let snapshot = parse_document(&item.0, "input_snapshot_json")?;
    let result = parse_document(&item.0, "result_json")?;
    let components = components(&result, false)?;
    let allocation = allocate_item(&item.0, &snapshot, &components)?;
    Ok(allocation
        .units
        .iter()
        .zip(allocation.net)
        .filter(|(_, n)| *n != 0.0 && !n.is_nan())
        .map(|(u, n)| PayrollCostShare {
            amount_minor: n,
            dimensions: u.dims(),
        })
        .collect())
}
pub fn payroll_net_cost_shares(
    item: &PayrollProjectionItem,
) -> PayrollProjectionOutcome<Vec<PayrollCostShare>> {
    shares_inner(item).into()
}
fn account(accounts: &Operand, id: &str, code: &str) -> R<Text> {
    let value = accounts.prop(id)?;
    if !value.truthy() {
        return Err(Fault::business(
            422,
            "PAYROLL_ACCOUNT_MAPPING_REQUIRED",
            format!("Map a ledger account for {code} before posting payroll."),
        ));
    }
    as_text(&value, id)
}
fn fallback(accounts: &Operand, id: &str, other: &str) -> R<Operand> {
    let value = accounts.prop(id)?;
    if value.nullish() {
        accounts.prop(other)
    } else {
        Ok(value)
    }
}
fn ledger_value(value: &Operand, code: &str) -> R<Text> {
    if !value.truthy() {
        return Err(Fault::business(
            422,
            "PAYROLL_ACCOUNT_MAPPING_REQUIRED",
            format!("Map a ledger account for {code} before posting payroll."),
        ));
    }
    as_text(value, code)
}
struct Group {
    account: Text,
    description: String,
    debit: f64,
    credit: f64,
    dimensions: PayrollDimensions,
}
fn add(
    groups: &mut Vec<Group>,
    totals: &mut PayrollTotals,
    account: Text,
    description: String,
    debit: f64,
    credit: f64,
    dimensions: PayrollDimensions,
) {
    if (debit == 0.0 || debit.is_nan()) && (credit == 0.0 || credit.is_nan()) {
        return;
    }
    if let Some(g) = groups.iter_mut().find(|g| {
        g.account == account && g.description == description && g.dimensions == dimensions
    }) {
        g.debit += debit;
        g.credit += credit
    } else {
        groups.push(Group {
            account,
            description,
            debit,
            credit,
            dimensions,
        })
    }
    totals.debit_minor += debit;
    totals.credit_minor += credit
}
fn decimal(n: f64) -> String {
    let negative = n < 0.0;
    let absolute = n.abs();
    let whole = (absolute / 100.0).floor();
    let fraction = js_print(absolute % 100.0);
    format!(
        "{}{}.{:0>2}",
        if negative { "-" } else { "" },
        js_print(whole),
        fraction
    )
}
fn category_add(categories: &mut Categories, key: String, value: f64) {
    if let Some((_, n)) = categories.0.iter_mut().find(|(k, _)| *k == key) {
        *n += value
    } else {
        categories.0.push((key, 0.0 + value))
    }
}
fn lines_inner(
    items: &[PayrollProjectionItem],
    accounts: &PayrollProjectionAccounts,
) -> R<PayrollAccountingProjection> {
    let mut groups = Vec::new();
    let mut totals = PayrollTotals::default();
    let mut categories = Categories(Vec::new());
    for item in items {
        let snapshot = parse_document(&item.0, "input_snapshot_json")?;
        let result = parse_document(&item.0, "result_json")?;
        let components = components(&result, true)?;
        let mut earnings = Vec::new();
        let mut deductions = Vec::new();
        let mut employers = Vec::new();
        for c in &components {
            if kind(c, "earning")? {
                earnings.push(c.clone())
            } else if kind(c, "deduction")? {
                deductions.push(c.clone())
            } else if kind(c, "employer")? {
                employers.push(c.clone())
            }
        }
        let invalid = components.iter().any(|c| match c.prop("amountMinor") {
            Err(_) => true,
            Ok(x) => {
                !x.actual_safe_integer()
                    || match x.number() {
                        Ok(n) => n < 0.0,
                        Err(_) => true,
                    }
            }
        });
        if invalid {
            return Err(fail(
                "PAYROLL_COMPONENTS_INVALID",
                "Payroll item components do not agree with the approved totals.",
            ));
        }
        let deduction_sum = source_number_sum(
            &deductions
                .iter()
                .map(|c| c.prop("amountMinor"))
                .collect::<R<Vec<_>>>()?,
        )?;
        let deduction_total = item.0.prop("deductions_minor")?.number()?;
        if deduction_sum != deduction_total {
            return Err(fail(
                "PAYROLL_COMPONENTS_INVALID",
                "Payroll item components do not agree with the approved totals.",
            ));
        }
        let employer_sum = source_number_sum(
            &employers
                .iter()
                .map(|c| c.prop("amountMinor"))
                .collect::<R<Vec<_>>>()?,
        )?;
        let employer_total = item.0.prop("employer_cost_minor")?.number()?;
        if employer_sum != employer_total {
            return Err(fail(
                "PAYROLL_COMPONENTS_INVALID",
                "Payroll item components do not agree with the approved totals.",
            ));
        }
        let gross = item.0.prop("gross_minor")?.number()?;
        let net = item.0.prop("net_minor")?.number()?;
        let wage = gross
            - source_number_sum(
                &earnings
                    .iter()
                    .map(|c| c.prop("amountMinor"))
                    .collect::<R<Vec<_>>>()?,
            )?;
        if wage < 0.0 || net != gross - deduction_total {
            return Err(fail(
                "PAYROLL_TOTALS_INVALID",
                "Payroll item totals do not balance.",
            ));
        }
        let a = allocate_item(&item.0, &snapshot, &components)?;
        for (i, u) in a.units.iter().enumerate() {
            let dims = u.dims();
            let wage_account = as_text(&accounts.0.prop("expenseAccountId")?, "expenseAccountId")?;
            add(
                &mut groups,
                &mut totals,
                wage_account.clone(),
                "Payroll wages".into(),
                a.wages[i],
                0.0,
                dims.clone(),
            );
            let ot = as_text(
                &fallback(&accounts.0, "overtimeExpenseAccountId", "expenseAccountId")?,
                "overtimeExpenseAccountId",
            )?;
            add(
                &mut groups,
                &mut totals,
                ot,
                "Payroll overtime".into(),
                a.overtime[i],
                0.0,
                dims.clone(),
            );
            for (j, c) in earnings.iter().enumerate() {
                let k = category(c, "earning")?;
                let id = match k.as_str() {
                    "service_charge" => account(&accounts.0, "serviceChargeExpenseAccountId", &k)?,
                    "bonus" => as_text(
                        &fallback(&accounts.0, "bonusExpenseAccountId", "expenseAccountId")?,
                        "bonusExpenseAccountId",
                    )?,
                    "overtime" => as_text(
                        &fallback(&accounts.0, "overtimeExpenseAccountId", "expenseAccountId")?,
                        "overtimeExpenseAccountId",
                    )?,
                    _ => wage_account.clone(),
                };
                add(
                    &mut groups,
                    &mut totals,
                    id,
                    format!("Payroll {}", k.replace('_', " ")),
                    a.earnings[j][i],
                    0.0,
                    dims.clone(),
                );
            }
            for (j, c) in deductions.iter().enumerate() {
                let k = category(c, "deduction")?;
                let id = match k.as_str() {
                    "tax" => account(&accounts.0, "taxPayableAccountId", &k)?,
                    "social_security" => ledger_value(
                        &fallback(
                            &accounts.0,
                            "socialSecurityPayableAccountId",
                            "employerContributionAccountId",
                        )?,
                        &k,
                    )?,
                    "advance" => account(&accounts.0, "advanceReceivableAccountId", &k)?,
                    "benefit" => account(&accounts.0, "benefitPayableAccountId", &k)?,
                    _ => account(&accounts.0, "otherDeductionPayableAccountId", &k)?,
                };
                add(
                    &mut groups,
                    &mut totals,
                    id,
                    format!("Payroll {}", k.replace('_', " ")),
                    0.0,
                    a.deductions[j][i],
                    dims.clone(),
                );
            }
            for (j, c) in employers.iter().enumerate() {
                let k = category(c, "employer")?;
                let id = match k.as_str() {
                    "social_security" => ledger_value(
                        &fallback(
                            &accounts.0,
                            "socialSecurityPayableAccountId",
                            "employerContributionAccountId",
                        )?,
                        &k,
                    )?,
                    "benefit" => account(&accounts.0, "benefitPayableAccountId", &k)?,
                    _ => account(&accounts.0, "employerContributionAccountId", &k)?,
                };
                let expense = account(
                    &accounts.0,
                    "employerExpenseAccountId",
                    &format!("employer_{k}_expense"),
                )?;
                if expense == wage_account {
                    return Err(Fault::business(
                        422,
                        "PAYROLL_ACCOUNT_MAPPING_REQUIRED",
                        "Map employer costs to an expense account separate from wages.",
                    ));
                }
                add(
                    &mut groups,
                    &mut totals,
                    expense,
                    format!("Payroll employer {} expense", k.replace('_', " ")),
                    a.employers[j][i],
                    0.0,
                    dims.clone(),
                );
                add(
                    &mut groups,
                    &mut totals,
                    id,
                    format!("Payroll {}", k.replace('_', " ")),
                    0.0,
                    a.employers[j][i],
                    dims.clone(),
                );
            }
            let net_account = as_text(
                &accounts.0.prop("netPayableAccountId")?,
                "netPayableAccountId",
            )?;
            add(
                &mut groups,
                &mut totals,
                net_account,
                "Net payroll payable".into(),
                0.0,
                a.net[i],
                dims,
            );
        }
        for c in &earnings {
            category_add(
                &mut categories,
                category(c, "earning")?,
                amount_component(c)?,
            )
        }
        for c in &deductions {
            category_add(
                &mut categories,
                category(c, "deduction")?,
                amount_component(c)?,
            )
        }
        for c in &employers {
            category_add(
                &mut categories,
                format!("employer_{}", category(c, "employer")?),
                amount_component(c)?,
            )
        }
        totals.gross_minor += gross;
        totals.deductions_minor += deduction_total;
        totals.employer_minor += employer_total;
        totals.net_minor += net;
    }
    if items.is_empty()
        || totals.net_minor <= 0.0
        || totals.debit_minor != totals.credit_minor
        || !safe(totals.debit_minor)
    {
        return Err(fail(
            "PAYROLL_TOTALS_INVALID",
            "Payroll accrual debits and credits must balance.",
        ));
    }
    let lines = groups
        .into_iter()
        .map(|g| PayrollJournalLine {
            account_id: g.account,
            description: g.description,
            debit: decimal(g.debit),
            credit: decimal(g.credit),
            dimensions: g.dimensions,
        })
        .collect::<Vec<_>>();
    if lines.len() > 500 {
        return Err(Fault::business(
            422,
            "PAYROLL_JOURNAL_TOO_LARGE",
            "Split this run before posting more than 500 grouped ledger lines.",
        ));
    }
    Ok(PayrollAccountingProjection {
        lines,
        totals,
        categories,
    })
}
pub fn payroll_accounting_lines(
    items: &[PayrollProjectionItem],
    accounts: &PayrollProjectionAccounts,
) -> PayrollProjectionOutcome<PayrollAccountingProjection> {
    lines_inner(items, accounts).into()
}
