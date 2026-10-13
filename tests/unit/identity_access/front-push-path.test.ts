import { describe, expect, it } from "vitest";
import { frontPushLandingPath } from "../../../src/lib/shared/front-host";

describe("Front push landing path", () => {
  it("stays on the Front origin and preserves each authorized scope", () => {
    expect(frontPushLandingPath({ kind: "holding", holdingSlug: "acme" }, "hotel"))
      .toBe("/acme/hotel/holding/notifications");
    expect(frontPushLandingPath({ kind: "branch", holdingSlug: "acme", branchSlug: "bangkok" }, "hotel"))
      .toBe("/acme/hotel/branches/bangkok/notifications");
    expect(frontPushLandingPath({ kind: "department", holdingSlug: "acme", branchSlug: "bangkok", departmentSlug: "rooms" }, "hotel"))
      .toBe("/acme/hotel/branches/bangkok/departments/rooms/notifications");
    expect(frontPushLandingPath({ kind: "department", holdingSlug: "acme", departmentSlug: "rooms" }, "hotel"))
      .toBe("/acme/hotel/departments/rooms/notifications");
  });
});
