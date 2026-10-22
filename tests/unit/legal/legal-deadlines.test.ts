import { it, expect } from "vitest";
import { previewDeadline } from "@erpx/server-legal/public/rules/deadlines";
it("uses explicit calendar rules across month ends, leap years and weekends", () => {
  expect(
    previewDeadline({ start: "2028-01-31", amount: 1, unit: "months" }).date,
  ).toBe("2028-02-29");
  expect(
    previewDeadline({ start: "2027-01-31", amount: 1, unit: "months" }).date,
  ).toBe("2027-02-28");
  expect(
    previewDeadline({ start: "2028-02-28", amount: 2, unit: "calendar_days" })
      .date,
  ).toBe("2028-03-01");
  expect(
    previewDeadline({
      start: "2026-09-18",
      amount: 1,
      unit: "business_days",
      holidays: ["2026-09-21"],
    }).date,
  ).toBe("2026-09-22");
  expect(
    previewDeadline({ start: "2026-09-19", amount: 0, unit: "calendar_days" }),
  ).toMatchObject({ date: "2026-09-19", requiresReview: true });
  expect(() =>
    previewDeadline({ start: "2027-02-29", amount: 1, unit: "calendar_days" }),
  ).toThrow();
});
