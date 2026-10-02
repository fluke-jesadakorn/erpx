import { describe, expect, it } from "vitest";
import { peopleContextHref } from "../src/lib/people-route-context";

describe("People legacy context", () => {
  it("retains repeated filters, roster request, and week through canonical navigation", () => {
    const href = peopleContextHref("/holdings/acme", {
      department: "operations", week: "2026-09-28", view: "mine", request: "request-1",
      filter: ["leave", "shift"], ignored: undefined,
    }, { panel: "schedule" });
    const url = new URL(href, "https://people.invalid");
    expect(url.pathname).toBe("/holdings/acme/people");
    expect(url.searchParams.get("panel")).toBe("schedule");
    expect(url.searchParams.get("request")).toBe("request-1");
    expect(url.searchParams.get("week")).toBe("2026-09-28");
    expect(url.searchParams.getAll("filter")).toEqual(["leave", "shift"]);
    expect(url.searchParams.has("ignored")).toBe(false);
  });

  it("keeps pending moves without inventing a destination and scopes draft review", () => {
    const pending = new URL(peopleContextHref("/holdings/acme", { kind: "person", id: "person-1", draft: "private-1" }, { panel: "move" }), "https://people.invalid");
    expect(pending.searchParams.get("kind")).toBe("person");
    expect(pending.searchParams.has("targetId")).toBe(false);
    const review = new URL(peopleContextHref("/holdings/acme/branches/main", { panel: "move", draft: "other", q: "สมชาย" }, { panel: "review", draft: "private-1" }), "https://people.invalid");
    expect(review.pathname).toBe("/holdings/acme/branches/main/people");
    expect(review.searchParams.getAll("draft")).toEqual(["private-1"]);
    expect(review.searchParams.get("q")).toBe("สมชาย");
  });
});
