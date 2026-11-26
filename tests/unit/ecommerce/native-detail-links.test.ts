import { describe, it, expect, vi } from "vitest";

vi.mock("server-only", () => ({}));
import {
  createDetailLinkEcomClient,
  nativeDetailLinksEcomConfigured,
  type DetailLinkHoldingContext,
  type DetailLinkTransport,
} from "../../../packages/server-ecommerce/src/adapters/native-detail-links/client";

/** Exact dev value for ERPX_NATIVE_ECOM_DETAIL_LINKS_CONFIG (1..64 holdings). */
export const DEV_CONFIG = {
  privateBaseUrl: "http://127.0.0.1:3199",
  timeoutMs: 10000,
  holdings: [
    {
      origin: "https://acme.jesadakorn.com",
      host: "acme.jesadakorn.com",
      holdingId: "h001",
      holdingSlug: "acme",
    },
    {
      origin: "https://northstar.jesadakorn.com",
      host: "northstar.jesadakorn.com",
      holdingId: "h002",
      holdingSlug: "northstar",
    },
    {
      origin: "https://erpx-internal.jesadakorn.com",
      host: "erpx-internal.jesadakorn.com",
      holdingId: "h003",
      holdingSlug: "erpx",
    },
  ],
} as const;

const raw = JSON.stringify(DEV_CONFIG);
const acme: DetailLinkHoldingContext = { holdingId: "h001", holdingSlug: "acme" };
const fulfillment = {
  id: "link-1",
  checkout_id: "checkout-1",
  fulfillment_id: "fulfillment-1",
  source_order_version: 2,
  status: "confirmed",
  created_at: 42,
};
const returns = {
  id: "return-link-1",
  checkout_id: "checkout-1",
  current_return_status: "approved",
  current_return_version: 3,
};
const links = { fulfillmentLinks: [fulfillment], returnLinks: [returns] };
const ok = (value: unknown) =>
  new Response(JSON.stringify(value), { status: 200, headers: { "content-type": "application/json" } });

describe("Ecommerce native checkout detail-link transport", () => {
  it("posts only the checkout identity to the exact owner route", async () => {
    const transport: DetailLinkTransport = vi.fn(async (url, init) => {
      expect(url.toString()).toBe(
        "http://127.0.0.1:3199/api/v1/holdings/acme/ecommerce/native-detail-links",
      );
      expect(init.method).toBe("POST");
      expect(init.body).toBe(JSON.stringify({ checkoutId: "checkout-1" }));
      const headers = init.headers as Headers;
      expect(headers.get("host")).toBe("acme.jesadakorn.com");
      expect(headers.get("origin")).toBe("https://acme.jesadakorn.com");
      return ok(links);
    });
    expect(await createDetailLinkEcomClient(raw, transport).links("checkout-1", acme)).toEqual(links);
  });

  it("routes every registered holding by holdingId + holdingSlug", async () => {
    for (const [actor, slug, host] of [
      [{ holdingId: "h002", holdingSlug: "northstar" }, "northstar", "northstar.jesadakorn.com"],
      [{ holdingId: "h003", holdingSlug: "erpx" }, "erpx", "erpx-internal.jesadakorn.com"],
    ] as const) {
      const transport: DetailLinkTransport = vi.fn(async (url, init) => {
        expect(url.toString()).toBe(
          `http://127.0.0.1:3199/api/v1/holdings/${slug}/ecommerce/native-detail-links`,
        );
        expect((init.headers as Headers).get("host")).toBe(host);
        return ok(links);
      });
      expect(await createDetailLinkEcomClient(raw, transport).links("checkout-1", actor)).toEqual(links);
    }
  });

  it("never falls back to the first holding for an unmatched or absent WorkContext", async () => {
    const transport: DetailLinkTransport = vi.fn(async () => ok(links));
    const client = createDetailLinkEcomClient(raw, transport);
    for (const actor of [
      undefined,
      { holdingId: "h404", holdingSlug: "acme" },
      { holdingId: "h001", holdingSlug: "northstar" },
    ]) {
      await expect(client.links("checkout-1", actor)).rejects.toMatchObject({
        status: 503,
        code: "ECOMMERCE_DETAIL_LINKS_TARGET_UNAVAILABLE",
      });
    }
    expect(transport).not.toHaveBeenCalled();
  });

  it("fails closed on missing config, bad identity and malformed envelopes", async () => {
    const transport: DetailLinkTransport = vi.fn(async () => ok(links));
    expect(() => createDetailLinkEcomClient(undefined, transport)).toThrow(
      "The native Ecommerce detail link service is unavailable.",
    );
    await expect(createDetailLinkEcomClient(raw, transport).links("bad id", acme)).rejects.toMatchObject({
      code: "ECOMMERCE_DETAIL_LINKS_REQUEST_INVALID",
    });
    for (const body of [
      { ...links, fulfillmentLinks: [{ ...fulfillment, unexpected: true }] },
      { ...links, returnLinks: [{ id: "return-link-1" }] },
      { ...links, returnLinks: "not-an-array" },
      { fulfillmentLinks: [] },
    ]) {
      const client = createDetailLinkEcomClient(raw, vi.fn(async () => ok(body)));
      await expect(client.links("checkout-1", acme)).rejects.toMatchObject({
        status: 503,
        code: "ECOMMERCE_DETAIL_LINKS_RESPONSE_INVALID",
      });
    }
    expect(transport).not.toHaveBeenCalled();
  });

  it("preserves every retired row field and reports configuration presence", () => {
    expect(Object.keys(links.fulfillmentLinks[0])).toHaveLength(6);
    expect(Object.keys(links.returnLinks[0])).toHaveLength(4);
    expect(nativeDetailLinksEcomConfigured({ [ "ERPX_NATIVE_ECOM_DETAIL_LINKS_CONFIG" ]: raw })).toBe(true);
    expect(nativeDetailLinksEcomConfigured({})).toBe(false);
  });
});
