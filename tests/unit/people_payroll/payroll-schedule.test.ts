import { describe, expect, test } from "vitest";
import { payrollPeriodForDate } from "../../../src/lib/people_payroll/payroll-schedule";

describe("payroll schedule calendar", () => {
  test("monthly pay follows the close, including February in a leap year", () => {
    expect(payrollPeriodForDate({frequency:"monthly",schedule_anchor_date:null,pay_delay_days:3},"2028-02-29"))
      .toEqual({periodStart:"2028-02-01",periodEnd:"2028-02-29",payDate:"2028-03-03"});
  });
  test("semimonthly periods split after the 15th", () => {
    const schedule={frequency:"semimonthly" as const,schedule_anchor_date:null,pay_delay_days:1};
    expect(payrollPeriodForDate(schedule,"2026-09-15")).toEqual({periodStart:"2026-09-01",periodEnd:"2026-09-15",payDate:"2026-09-16"});
    expect(payrollPeriodForDate(schedule,"2026-09-16")).toEqual({periodStart:"2026-09-16",periodEnd:"2026-09-30",payDate:"2026-10-01"});
  });
  test("anchored fortnightly schedules keep the same boundaries before and after anchor", () => {
    const schedule={frequency:"fortnightly" as const,schedule_anchor_date:"2026-09-07",pay_delay_days:2};
    expect(payrollPeriodForDate(schedule,"2026-09-06")).toEqual({periodStart:"2026-08-24",periodEnd:"2026-09-06",payDate:"2026-09-08"});
    expect(payrollPeriodForDate(schedule,"2026-09-07")).toEqual({periodStart:"2026-09-07",periodEnd:"2026-09-20",payDate:"2026-09-22"});
  });
});
