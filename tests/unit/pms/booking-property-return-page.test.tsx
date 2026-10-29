import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { isValidElement, type ComponentProps, type ReactElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { AppRouterContext, type AppRouterInstance } from "next/dist/shared/lib/app-router-context.shared-runtime";
import type { HotelCard } from "@erpx/feature-pms/components/booking-flow";
import type { BookingPaymentReturnStatus } from "@erpx/feature-pms/components/booking-payment-return";

// Only the named native owner ports are substituted. An explicit framework
// navigation IO port mounts real Next context; the owner, notFound, product
// routing helpers and both Booking presentations execute unchanged.
type PaymentSnapshot = BookingPaymentReturnStatus & { hotel: Pick<HotelCard, "id"> };
type CatalogReader = () => Promise<HotelCard[]>;
type PaymentReader = (attempt: string) => Promise<PaymentSnapshot>;
const owner = vi.hoisted(() => ({
  catalog: vi.fn<CatalogReader>(),
  payment: vi.fn<PaymentReader>(),
  catalogQueries: vi.fn<(slug: string) => Promise<{ bookingCatalog: CatalogReader }>>(),
  paymentQueries: vi.fn<(slug: string) => Promise<{ bookingPaymentStatus: PaymentReader }>>(),
}));
vi.mock("@erpx/server-pms/public/presentation-context-queries", () => ({
  bookingPresentationQueries: owner.catalogQueries,
  bookingPaymentPresentationQueries: owner.paymentQueries,
}));
import PropertyPage from "@erpx/feature-booking/routes/book/[holdingSlug]/[propertyId]/page";
import { BookingPaymentReturn } from "@erpx/feature-pms/components/booking-payment-return";
import { ProductLinkEnvironment } from "@erpx/workplace/components/product-link-context";
import { ApiError } from "@erpx/server-runtime/public/http";

const target = { holdingSlug: "original-holding", propertyId: "original-property" };
const attempt = "original-payment.original-signature";
const hotel: HotelCard = {
  id: target.propertyId, name: "Original published hotel", description: "Original property description",
  address: "Original address", contactEmail: "hotel@example.test", contactPhone: "000",
  currency: "THB", checkIn: "14:00", checkOut: "12:00", timezone: "Asia/Bangkok", images: [], roomContent: [],
};
const initial: BookingPaymentReturnStatus = {
  number: "ORIGINAL-BOOKING-001", state: "confirmed", paymentState: "received",
  checkoutUrl: null, managementToken: "original-management.private-signature", refundState: null,
};
const paymentSnapshot = (): PaymentSnapshot => ({ ...initial, hotel: { id: target.propertyId } });
const page = (query: { attempt?: string | string[] } = {}) => PropertyPage({
  params: Promise.resolve(target), searchParams: Promise.resolve(query),
});
const frameworkRouter = { back:vi.fn(),forward:vi.fn(),refresh:vi.fn(),push:vi.fn(),replace:vi.fn(),prefetch:vi.fn(),bfcacheId:"booking-property-unit" } satisfies AppRouterInstance;
const html = (node: ReactElement) => renderToStaticMarkup(<AppRouterContext.Provider value={frameworkRouter}><ProductLinkEnvironment audience="booking">{node}</ProductLinkEnvironment></AppRouterContext.Provider>);
const notFound = { digest: "NEXT_HTTP_ERROR_FALLBACK;404" };
beforeEach(() => {
  vi.resetAllMocks();
  vi.stubEnv("PMS_STRIPE_BOOKING_ENABLED", "true");
  owner.catalogQueries.mockResolvedValue({ bookingCatalog: owner.catalog });
  owner.paymentQueries.mockResolvedValue({ bookingPaymentStatus: owner.payment });
  owner.catalog.mockResolvedValue([hotel]);
  owner.payment.mockResolvedValue(paymentSnapshot());
});
afterEach(() => {
  for(const operation of Object.values(frameworkRouter))if(typeof operation==="function")expect(operation).not.toHaveBeenCalled();
  vi.unstubAllEnvs();
});

describe("Booking property Server Component owner selection", () => {
  it("renders the published property without acquiring a payment owner", async () => {
    owner.paymentQueries.mockRejectedValueOnce(Error("Payment owner must remain unused for catalogue requests"));
    const rendered = html(await page());
    expect(rendered).toContain(hotel.name);
    expect(rendered).toContain(hotel.description);
    expect(rendered).not.toContain('aria-label="Booking payment status"');
    expect(owner.catalogQueries).toHaveBeenCalledExactlyOnceWith(target.holdingSlug);
    expect(owner.catalog).toHaveBeenCalledExactlyOnceWith();
    expect(owner.paymentQueries).not.toHaveBeenCalled();
    expect(owner.payment).not.toHaveBeenCalled();
  });

  it("renders a signed payment return without requiring public catalogue publication and binds the original property", async () => {
    owner.catalogQueries.mockRejectedValueOnce(Error("Catalogue owner must remain unused for payment returns"));
    const node = await page({ attempt });
    const paymentNode = node.props.children;
    expect(isValidElement<ComponentProps<typeof BookingPaymentReturn>>(paymentNode)).toBe(true);
    if (!isValidElement<ComponentProps<typeof BookingPaymentReturn>>(paymentNode)) throw Error("The actual payment presentation is required");
    expect(paymentNode.type).toBe(BookingPaymentReturn);
    expect(paymentNode.props).toEqual({ slug: target.holdingSlug, propertyId: target.propertyId, attempt, initial });
    const rendered = html(node);
    expect(rendered).toContain('aria-label="Booking payment status"');
    expect(rendered).toContain(`Booking confirmed · ${initial.number}`);
    expect(rendered).toContain(`/book/${target.holdingSlug}/manage#token=${initial.managementToken}`);
    expect(rendered).not.toContain(hotel.name);
    expect(owner.paymentQueries).toHaveBeenCalledExactlyOnceWith(target.holdingSlug);
    expect(owner.payment).toHaveBeenCalledExactlyOnceWith(attempt);
    expect(owner.catalogQueries).not.toHaveBeenCalled();
    expect(owner.catalog).not.toHaveBeenCalled();
  });

  it.each([
    { label: "empty token", token: "" },
    { label: "repeated query token", token: [attempt, "another-payment.signature"] },
    { label: "oversized token", token: "x".repeat(1001) },
  ])("refuses $label before consulting payment status", async ({ token }) => {
    await expect(page({ attempt: token })).rejects.toMatchObject(notFound);
    expect(owner.paymentQueries).toHaveBeenCalledExactlyOnceWith(target.holdingSlug);
    expect(owner.payment).not.toHaveBeenCalled();
    expect(owner.catalogQueries).not.toHaveBeenCalled();
    expect(owner.catalog).not.toHaveBeenCalled();
  });

  it.each([401, 403, 404])("uses real Next notFound for a current payment owner refusal %i without catalogue fallback", async status => {
    owner.payment.mockRejectedValueOnce(new ApiError(status, "PMS_PAYMENT_UNAVAILABLE", "Original payment owner refusal"));
    await expect(page({ attempt })).rejects.toMatchObject(notFound);
    expect(owner.payment).toHaveBeenCalledExactlyOnceWith(attempt);
    expect(owner.catalogQueries).not.toHaveBeenCalled();
    expect(owner.catalog).not.toHaveBeenCalled();
  });

  it("refuses a valid signed payment belonging to another actual property without catalogue fallback", async () => {
    owner.payment.mockResolvedValueOnce({ ...paymentSnapshot(), hotel: { id: "another-property" } });
    await expect(page({ attempt })).rejects.toMatchObject(notFound);
    expect(owner.payment).toHaveBeenCalledExactlyOnceWith(attempt);
    expect(owner.catalogQueries).not.toHaveBeenCalled();
    expect(owner.catalog).not.toHaveBeenCalled();
  });

  it.each(["catalogue", "payment"] as const)("uses real Next notFound when the %s owner cannot resolve the Holding", async mode => {
    const resource = mode === "catalogue" ? owner.catalogQueries : owner.paymentQueries;
    resource.mockRejectedValueOnce(Error("HOLDING_NOT_FOUND"));
    await expect(page(mode === "catalogue" ? {} : { attempt })).rejects.toMatchObject(notFound);
    expect(owner.catalog).not.toHaveBeenCalled();
    expect(owner.payment).not.toHaveBeenCalled();
  });

  it.each(["catalogue", "payment"] as const)("preserves an unexpected %s resource failure by identity", async mode => {
    const failure = Error("Original owner resource unavailable");
    (mode === "catalogue" ? owner.catalogQueries : owner.paymentQueries).mockRejectedValueOnce(failure);
    await expect(page(mode === "catalogue" ? {} : { attempt })).rejects.toBe(failure);
    expect(owner.catalog).not.toHaveBeenCalled();
    expect(owner.payment).not.toHaveBeenCalled();
  });

  it("preserves an unexpected catalogue reader failure by identity", async () => {
    const failure = Error("Original catalogue unavailable");
    owner.catalog.mockRejectedValueOnce(failure);
    await expect(page()).rejects.toBe(failure);
    expect(owner.paymentQueries).not.toHaveBeenCalled();
  });

  it.each([Error("Unexpected payment failure"), new ApiError(409, "PMS_PAYMENT_MISMATCH", "Original Finance review needed"), new ApiError(502, "PMS_PROVIDER_UNAVAILABLE", "Original provider unavailable")])("preserves an unexpected payment owner failure by identity (%s)", async failure => {
    owner.payment.mockRejectedValueOnce(failure);
    await expect(page({ attempt })).rejects.toBe(failure);
    expect(owner.payment).toHaveBeenCalledExactlyOnceWith(attempt);
    expect(owner.catalogQueries).not.toHaveBeenCalled();
    expect(owner.catalog).not.toHaveBeenCalled();
  });

  it("keeps the original disabled-Booking gate before either owner is acquired", async () => {
    vi.stubEnv("PMS_STRIPE_BOOKING_ENABLED", "false");
    await expect(page({ attempt })).rejects.toMatchObject(notFound);
    expect(owner.catalogQueries).not.toHaveBeenCalled();
    expect(owner.paymentQueries).not.toHaveBeenCalled();
    expect(owner.catalog).not.toHaveBeenCalled();
    expect(owner.payment).not.toHaveBeenCalled();
  });
});
