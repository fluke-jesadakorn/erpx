import { expect, it } from "vitest";
import { frontActionItems, frontCompanyItems } from "../../../src/server/modules/work_projection/public/front-quick-access";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import { frontText } from "../../../src/lib/shared/front-language";

function actor(permissions: string[]): WorkContext {
  return { userId: "staff-1", role: "staff", permissions, scope: { kind: "holding" } } as unknown as WorkContext;
}

it("offers only authorized local POS tabs and keeps them on Front", () => {
  const items = frontActionItems("pos", actor(["pos.read", "pos.sell"]), "/acme/hotel/holding/");
  expect(items.map((item) => item.id)).toEqual(["sale", "shifts", "history"]);
  expect(items.map((item) => item.href)).toEqual([
    "/acme/hotel/holding/?tab=sale",
    "/acme/hotel/holding/?tab=shifts",
    "/acme/hotel/holding/?tab=history",
  ]);
  expect(items.every((item) => !item.href.includes("/holdings/"))).toBe(true);
});

it("offers only authorized PMS sections", () => {
  const items = frontActionItems("pms", actor(["pms.read", "pms.frontdesk", "pms.housekeeping"]), "/acme/hotel/holding/");
  expect(items.map((item) => item.id)).toEqual(["dashboard", "front-desk", "rooms", "housekeeping"]);
  expect(items.map((item) => item.href)).toEqual([
    "/acme/hotel/holding/",
    "/acme/hotel/holding/front-desk",
    "/acme/hotel/holding/rooms",
    "/acme/hotel/holding/housekeeping",
  ]);
});

it("uses company choices as local Front links", () => {
  expect(frontCompanyItems([{ id: "company:acme:hotel", label: "Hotel", holding: "acme", slug: "hotel" }])).toMatchObject([
    { id: "company:acme:hotel", title: "Hotel", href: "/acme/hotel" },
  ]);
});

it("keeps Front action labels available in Thai and English", () => {
  expect(frontText("Cash shifts", "th")).toBe("กะเงินสด");
  expect(frontText("Cash shifts", "en")).toBe("Cash shifts");
  expect(frontText("Reservations", "th")).toBe("การจอง");
});
