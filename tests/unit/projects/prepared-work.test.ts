import { describe, expect, it } from "vitest";
import { preparedPostFields, preparedTaskFields } from "@erpx/feature-shared/lib/ai/prepared-work";
import { projectsClockCte } from "../../../packages/server-projects/src/adapters/clock-cte";

describe("guided intake details in owning forms", () => {
  it("only maps an unambiguous task owner and a real Gregorian date", () => {
    const people = [
      { user_id: "user-1", legal_name: "Alex" },
      { user_id: "user-2", legal_name: "Alex" },
      { user_id: "user-3", legal_name: "Mali" },
    ];
    expect(preparedTaskFields({ title: "  Repair display  ", description: "Check the cable",
      assignee: "Mali", dueDate: "2026-10-12" }, people)).toEqual({
      title: "Repair display", description: "Check the cable", assigneeId: "user-3", dueLocal: "2026-10-12T09:00",
    });
    expect(preparedTaskFields({ assignee: "Alex", dueDate: "2026-02-31" }, people)).toMatchObject({
      assigneeId: null, dueLocal: null,
    });
  });

  it("limits post preparation to message and recognized purpose", () => {
    expect(preparedPostFields({ body: "Team update", purpose: "handover", audience: "company" })).toEqual({
      body: "Team update", purpose: "handover",
    });
    expect(preparedPostFields({ body: "x".repeat(3100), purpose: "publish" })).toEqual({
      body: "x".repeat(3000), purpose: "update",
    });
  });
});

describe("projectsClockCte byte-identical read guards", () => {
  const clock = "floor(extract(epoch FROM clock_timestamp())*1000)::bigint";
  it("preserves the metric guard CTE", () => {
    expect(projectsClockCte("project_metric_clock")).toBe(
      `WITH project_metric_clock AS MATERIALIZED (SELECT ${clock} now_ms)`
    );
  });
  it("preserves the product-entry guard CTE", () => {
    expect(projectsClockCte("project_entry_clock")).toBe(
      `WITH project_entry_clock AS MATERIALIZED (SELECT ${clock} now_ms)`
    );
  });
});
