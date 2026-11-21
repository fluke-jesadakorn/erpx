//! Pure integer-only Billing target rating math. This mirrors the installed
//! TypeScript pricing port; it performs no provider, purchase, invoice or
//! assignment action and is not a target-period authority.
use erpx_server_shared::public::BusinessError;

pub const MAX_SAFE_INTEGER: i128 = 9_007_199_254_740_991;
const GB: i128 = 1_000_000_000;
const DAY: i64 = 86_400_000;
const BANGKOK_OFFSET: i64 = 7 * 60 * 60 * 1_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetPrice {
    pub id: String,
    pub effective_month: String,
    pub base_monthly_satang: i64,
    pub included_original_bytes: i64,
    pub storage_satang_per_gb_month: i64,
    pub ai_markup_percent: i64,
    pub seat_terms_status: String,
    pub included_seats: Option<i64>,
    pub seat_monthly_satang: Option<i64>,
    pub storage_allowance_mode: String,
    pub rating_enabled: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceWindow {
    pub start: i64,
    pub exact_start: i64,
    pub end: i64,
    pub service_days: i64,
    pub month_days: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeatRating {
    pub active_seat_milliseconds: i128,
    pub amount_satang: i64,
}

fn error(code: &'static str) -> BusinessError {
    BusinessError::invalid(code, "The target billing source is invalid.")
}

fn integer(value: i64, code: &'static str) -> Result<i128, BusinessError> {
    if value < 0 || i128::from(value) > MAX_SAFE_INTEGER {
        return Err(error(code));
    }
    Ok(i128::from(value))
}

fn checked(value: Option<i128>) -> Result<i128, BusinessError> {
    value.ok_or_else(|| error("TARGET_CHARGE_OUT_OF_RANGE"))
}

fn safe(value: i128) -> Result<i64, BusinessError> {
    if !(0..=MAX_SAFE_INTEGER).contains(&value) {
        return Err(error("TARGET_CHARGE_OUT_OF_RANGE"));
    }
    Ok(value as i64)
}

pub fn round_satang(numerator: i128, denominator: i128) -> Result<i64, BusinessError> {
    if numerator < 0 || denominator <= 0 {
        return Err(error("INVALID_TARGET_RATE"));
    }
    safe(checked(numerator.checked_add(denominator / 2))? / denominator)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = year - if month <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn parse_month(month: &str) -> Result<(i64, i64), BusinessError> {
    let bytes = month.as_bytes();
    if bytes.len() != 7
        || bytes[4] != b'-'
        || !bytes[0..4].iter().all(u8::is_ascii_digit)
        || !bytes[5..7].iter().all(u8::is_ascii_digit)
    {
        return Err(error("INVALID_BILLING_MONTH"));
    }
    let year: i64 = month[0..4]
        .parse()
        .map_err(|_| error("INVALID_BILLING_MONTH"))?;
    let number: i64 = month[5..7]
        .parse()
        .map_err(|_| error("INVALID_BILLING_MONTH"))?;
    // Date.UTC remaps 0..99; Billing uses explicit four-digit months.
    if !(0..=9999).contains(&year) || !(1..=12).contains(&number) {
        return Err(error("INVALID_BILLING_MONTH"));
    }
    Ok((year, number))
}

/// `monthBounds(month)`: UTC month edges shifted to Bangkok midnight.
pub fn month_bounds(month: &str) -> Result<(i64, i64), BusinessError> {
    let (year, number) = parse_month(month)?;
    // Date.UTC remaps years 0..99 to 1900..1999.
    let utc_year = if year < 100 { year + 1900 } else { year };
    let start_day = days_from_civil(utc_year, number, 1);
    let (next_year, next_number) = if number == 12 {
        (utc_year + 1, 1)
    } else {
        (utc_year, number + 1)
    };
    let end_day = days_from_civil(next_year, next_number, 1);
    Ok((
        start_day * DAY - BANGKOK_OFFSET,
        end_day * DAY - BANGKOK_OFFSET,
    ))
}

pub fn target_service_window(month: &str, started_at: i64) -> Result<ServiceWindow, BusinessError> {
    let (bounds_start, bounds_end) = month_bounds(month)?;
    if i128::from(started_at) > MAX_SAFE_INTEGER {
        return Err(error("INVALID_SERVICE_START"));
    }
    if started_at >= bounds_end {
        return Err(error("SERVICE_NOT_STARTED"));
    }
    let local_day = (started_at + BANGKOK_OFFSET).div_euclid(DAY);
    let start = (local_day * DAY - BANGKOK_OFFSET).max(bounds_start);
    Ok(ServiceWindow {
        start,
        exact_start: bounds_start.max(started_at),
        end: bounds_end,
        service_days: (bounds_end - start) / DAY,
        month_days: (bounds_end - bounds_start) / DAY,
    })
}

pub fn rate_target_base(
    price: &TargetPrice,
    month: &str,
    started_at: i64,
) -> Result<i64, BusinessError> {
    let window = target_service_window(month, started_at)?;
    round_satang(
        checked(
            integer(price.base_monthly_satang, "INVALID_BASE")?
                .checked_mul(i128::from(window.service_days)),
        )?,
        i128::from(window.month_days),
    )
}

pub fn rate_target_storage(
    price: &TargetPrice,
    month: &str,
    started_at: i64,
    original_byte_milliseconds: i128,
    paid_seat_milliseconds: i128,
) -> Result<i64, BusinessError> {
    if original_byte_milliseconds < 0 {
        return Err(error("INVALID_BYTE_TIME"));
    }
    if paid_seat_milliseconds < 0 {
        return Err(error("INVALID_SEAT_TIME"));
    }
    let window = target_service_window(month, started_at)?;
    let included = checked(
        integer(price.included_original_bytes, "INVALID_INCLUDED_STORAGE")?.checked_mul(
            if price.storage_allowance_mode == "per_paid_seat_time" {
                paid_seat_milliseconds
            } else {
                i128::from(window.end - window.exact_start)
            },
        ),
    )?;
    let excess = (original_byte_milliseconds - included).max(0);
    let (month_start, month_end) = month_bounds(month)?;
    round_satang(
        checked(excess.checked_mul(integer(
            price.storage_satang_per_gb_month,
            "INVALID_STORAGE_RATE",
        )?))?,
        GB * i128::from(month_end - month_start),
    )
}

pub fn rate_target_ai(
    price: &TargetPrice,
    verified_provider_cost_satang: i64,
) -> Result<i64, BusinessError> {
    let markup = integer(price.ai_markup_percent, "INVALID_AI_MARKUP")?;
    if markup > 100 {
        return Err(error("INVALID_AI_MARKUP"));
    }
    round_satang(
        checked(
            integer(verified_provider_cost_satang, "INVALID_PROVIDER_COST")?
                .checked_mul(100 + markup),
        )?,
        100,
    )
}

pub fn union_seat_intervals(intervals: &[(i64, i64)]) -> Result<Vec<(i64, i64)>, BusinessError> {
    let mut sorted = Vec::with_capacity(intervals.len());
    for (start, end) in intervals {
        if *end <= *start
            || i128::from(*start).abs() > MAX_SAFE_INTEGER
            || i128::from(*end).abs() > MAX_SAFE_INTEGER
        {
            return Err(error("INVALID_SEAT_INTERVAL"));
        }
        sorted.push((*start, *end));
    }
    sorted.sort_unstable();
    let mut merged: Vec<(i64, i64)> = Vec::new();
    for (start, end) in sorted {
        if let Some(last) = merged.last_mut()
            && start <= last.1
        {
            last.1 = last.1.max(end);
            continue;
        }
        merged.push((start, end));
    }
    Ok(merged)
}

pub fn rate_target_seats(
    price: &TargetPrice,
    month: &str,
    started_at: i64,
    users: &[Vec<(i64, i64)>],
) -> Result<SeatRating, BusinessError> {
    let seat_rate = price
        .seat_monthly_satang
        .ok_or_else(|| error("SEAT_PRICE_UNDEFINED"))?;
    if price.seat_terms_status != "defined" || price.included_seats != Some(0) {
        return Err(error("SEAT_PRICE_UNDEFINED"));
    }
    let window = target_service_window(month, started_at)?;
    let mut active = 0_i128;
    for intervals in users {
        for (start, end) in union_seat_intervals(intervals)? {
            let duration = (end.min(window.end) - start.max(window.exact_start)).max(0);
            active += i128::from(duration);
        }
    }
    let (month_start, month_end) = month_bounds(month)?;
    let amount = round_satang(
        checked(active.checked_mul(integer(seat_rate, "INVALID_SEAT_RATE")?))?,
        i128::from(month_end - month_start),
    )?;
    Ok(SeatRating {
        active_seat_milliseconds: active,
        amount_satang: amount,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    fn price() -> TargetPrice {
        TargetPrice {
            id: "price".into(),
            effective_month: "2026-10".into(),
            base_monthly_satang: 149_000,
            included_original_bytes: 20_000_000_000,
            storage_satang_per_gb_month: 500,
            ai_markup_percent: 10,
            seat_terms_status: "defined".into(),
            included_seats: Some(0),
            seat_monthly_satang: Some(49_900),
            storage_allowance_mode: "per_paid_seat_time".into(),
            rating_enabled: true,
        }
    }

    #[test]
    fn round_and_window_match_installed_pricing_rules() {
        assert_eq!(round_satang(10, 4).unwrap(), 3);
        assert_eq!(round_satang(5, 2).unwrap(), 3);
        assert_eq!(round_satang(0, 7).unwrap(), 0);
        assert!(round_satang(-1, 1).is_err());
        assert!(round_satang(1, 0).is_err());
        let window = target_service_window("2026-10", 1_791_000_000_000).unwrap();
        assert_eq!(window.month_days, 31);
        assert_eq!(window.service_days, 29);
        assert_eq!(window.start, 1_790_960_400_000);
        assert!(target_service_window("2026-10", 1_796_000_000_000).is_err());
        assert!(target_service_window("2026-10", -1).is_ok());
        assert!(month_bounds("2026-13").is_err());
        assert!(month_bounds("0001-01").is_ok());
    }

    #[test]
    fn base_storage_ai_and_seats_are_exact_integers() {
        let p = price();
        assert_eq!(
            rate_target_base(&p, "2026-10", 1_791_000_000_000).unwrap(),
            139_387
        );
        assert_eq!(
            rate_target_storage(
                &p,
                "2026-10",
                1_791_000_000_000,
                155_520_000_000_000_000_000,
                2_592_000_000,
            )
            .unwrap(),
            19_355
        );
        assert_eq!(rate_target_ai(&p, 1_000).unwrap(), 1_100);
        assert_eq!(
            union_seat_intervals(&[(0, 10), (5, 20), (30, 40)]).unwrap(),
            vec![(0, 20), (30, 40)]
        );
        assert!(union_seat_intervals(&[(10, 10)]).is_err());
        assert_eq!(
            union_seat_intervals(&[(-10, -5), (-7, 0)]).unwrap(),
            vec![(-10, 0)]
        );
        let mut undefined_seats = p.clone();
        undefined_seats.included_seats = None;
        assert!(
            rate_target_seats(
                &undefined_seats,
                "2026-10",
                1_791_000_000_000,
                &[vec![(1_790_956_800_000, 1_791_216_000_000)]],
            )
            .is_err()
        );
        let mut unsafe_price = p.clone();
        unsafe_price.base_monthly_satang = i64::MAX;
        assert!(rate_target_base(&unsafe_price, "2026-10", 1_791_000_000_000).is_err());
        let seats = rate_target_seats(
            &p,
            "2026-10",
            1_791_000_000_000,
            &[vec![(1_790_956_800_000, 1_791_216_000_000)]],
        )
        .unwrap();
        assert_eq!(seats.active_seat_milliseconds, 216_000_000);
        assert!(seats.amount_satang > 0);
    }

    #[test]
    fn pricing_matches_actual_typescript_oracle() {
        let cases = json!({
            "price": {
                "id": "price", "effectiveMonth": "2026-10",
                "baseMonthlySatang": 149000_i64, "includedOriginalBytes": 20000000000_i64,
                "storageSatangPerGbMonth": 500_i64, "aiMarkupPercent": 10_i64,
                "seatTermsStatus": "defined", "includedSeats": 0,
                "seatMonthlySatang": 49900_i64, "storageAllowanceMode": "per_paid_seat_time",
                "ratingEnabled": true
            },
            "start": 1791000000000_i64,
            "original": "155520000000000000000",
            "paidSeat": "2592000000",
            "provider": 1000_i64,
            "intervals": [[0, 10], [5, 20], [30, 40]]
        });
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let module = root.join("packages/feature-billing/src/lib/billing-target-pricing.ts");
        let script = r#"const m=await import(process.argv[1]);let data='';for await(const c of process.stdin)data+=c;const x=JSON.parse(data),p=x.price;const out={round:m.roundSatang(10n,4n),window:m.targetServiceWindow('2026-10',x.start),base:m.rateTargetBase(p,'2026-10',x.start),storage:m.rateTargetStorage(p,'2026-10',x.start,BigInt(x.original),BigInt(x.paidSeat)),ai:m.rateTargetAi(p,x.provider),merged:m.unionSeatIntervals(x.intervals),seats:m.rateTargetSeats(p,'2026-10',x.start,[[[1790956800000,1791216000000]]])};const big=(k,v)=>v;process.stdout.write(JSON.stringify(out,(k,v)=>typeof v==='bigint'?'BIG:'+v:v));"#;
        let mut child =
            std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required"))
                .current_dir(&root)
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
            "actual TS pricing oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let ts: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let p = price();
        assert_eq!(ts["round"], round_satang(10, 4).unwrap());
        let window = target_service_window("2026-10", 1_791_000_000_000).unwrap();
        assert_eq!(ts["window"]["serviceDays"], window.service_days);
        assert_eq!(ts["window"]["monthDays"], window.month_days);
        assert_eq!(
            ts["base"],
            rate_target_base(&p, "2026-10", 1_791_000_000_000).unwrap()
        );
        assert_eq!(
            ts["storage"],
            rate_target_storage(
                &p,
                "2026-10",
                1_791_000_000_000,
                155_520_000_000_000_000_000,
                2_592_000_000
            )
            .unwrap()
        );
        assert_eq!(ts["ai"], rate_target_ai(&p, 1_000).unwrap());
        assert_eq!(
            ts["merged"],
            json!(union_seat_intervals(&[(0, 10), (5, 20), (30, 40)]).unwrap())
        );
        let seats = rate_target_seats(
            &p,
            "2026-10",
            1_791_000_000_000,
            &[vec![(1_790_956_800_000, 1_791_216_000_000)]],
        )
        .unwrap();
        assert_eq!(
            ts["seats"]["activeSeatMilliseconds"],
            format!("BIG:{}", seats.active_seat_milliseconds)
        );
        assert_eq!(ts["seats"]["amountSatang"], seats.amount_satang);
    }
}
