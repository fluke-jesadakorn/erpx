import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { z } from "zod";
import {
  bookingCatalog,
  bookingDatabase,
  bookingEnv,
  bookingError,
  bookingRateLimit,
  bookingPaymentStatus,
  cancelPublicReservation,
  createPublicReservation,
  manageBooking,
  publicOffers,
  publicQuote,
  recoverBooking,
} from "@erpx/server-pms/public/pms-booking";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import {guestBookingDocument} from "@erpx/server-pms/public/pms/guest-document";
import { publicChangeQuote, confirmBookingChange } from "@erpx/server-pms/public/pms-booking-changes";
import { readBoundedPublicBookingBody } from "@erpx/server-pms/public/public-booking-body";
export const dynamic = "force-dynamic";
type Context = { params: Promise<{ holdingSlug: string; action: string }> };
export async function GET(request: Request, { params }: Context) {
  try {
    if (process.env.PMS_STRIPE_BOOKING_ENABLED === 'false')
      throw new ApiError(404, "BOOKING_COMING_SOON", "Online booking is not available in the pilot");
    const { holdingSlug, action } = await params;
    if(action==='reservations')return Response.json(canonicalDestinationLinks({data:await bookingPaymentStatus(await bookingDatabase(holdingSlug),new URL(request.url).searchParams.get('attempt')??'',await bookingEnv())}),{headers:{'Cache-Control':'no-store','Referrer-Policy':'no-referrer'}});
    if (action !== "catalog")
      throw new ApiError(404, "NOT_FOUND", "Page not found");
    return Response.json(
      canonicalDestinationLinks({ data: await bookingCatalog(await bookingDatabase(holdingSlug)) }),
      { headers: { "Cache-Control": "no-store" } },
    );
  } catch (e) {
    return errorResponse(bookingError(e));
  }
}
export async function POST(request: Request, { params }: Context) {
  try {
    if (process.env.PMS_STRIPE_BOOKING_ENABLED === 'false')
      throw new ApiError(404, "BOOKING_COMING_SOON", "Online booking is not available in the pilot");
    if (request.headers.get("origin") !== requestPublicOrigin(request))
      throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use the booking website");
    const { holdingSlug, action } = await params;
    if (
      ![
        "availability",
        "quote",
        "reservations",
        "manage",
        "cancel",
        "recover", "document",
        "change-quote", "change-confirm", "cancel-quote", "cancel-confirm",
      ].includes(action)
    )
      throw new ApiError(404, "NOT_FOUND", "Page not found");
    const text = await readBoundedPublicBookingBody(request);
    let body: unknown;
    try {
      body = JSON.parse(text);
    } catch {
      throw new ApiError(422, "INVALID_JSON", "Invalid request");
    }
    const db = await bookingDatabase(holdingSlug),
      env = await bookingEnv();
    const ip =
      request.headers.get("cf-connecting-ip") ??
      request.headers.get("x-forwarded-for")?.split(",")[0] ??
      "local";
    await bookingRateLimit(
      db,
      `public:${action}:${ip}`,
      ["reservations", "recover", "cancel", "change-confirm", "cancel-confirm"].includes(action) ? 5 : 60,
    );
    await bookingRateLimit(
      db,
      `global:${action}`,
      action === "recover" ? 100 : 1000,
    );
    const token =
      request.headers.get("authorization")?.replace(/^Bearer /, "") ?? "";
    let data: unknown;
    if (action === "availability") {
      const v = z
        .object({
          search: z.unknown(),
          propertyId: z.string().max(128).optional(),
        })
        .parse(body);
      data = await publicOffers(db, v.search, v.propertyId);
    } else if (action === "quote") {
      const v = z
        .object({ propertyId: z.string().min(1).max(128), rooms: z.unknown() })
        .parse(body);
      data = await publicQuote(db, v.propertyId, v.rooms, env);
    } else if (action === "reservations")
      data = await createPublicReservation(
        db,
        holdingSlug,
        body,
        request.headers.get("Idempotency-Key") ?? "",
        env,
      );
    else if (action === "manage") data = await manageBooking(db, token);
    else if(action === "document")data=await guestBookingDocument(db,token,body);
    else if (action === "change-quote" || action === "cancel-quote") data = await publicChangeQuote(db, token, action === "change-quote" ? "change" : "cancel", body, env);
    else if (action === "change-confirm" || action === "cancel-confirm") {
      const v = z.object({proposalToken:z.string().min(1).max(100000)}).parse(body);
      data = await confirmBookingChange(db, holdingSlug, token, v.proposalToken, request.headers.get("Idempotency-Key") ?? "", env,Date.now(),undefined,action.startsWith("cancel")?"cancel":"change");
    }
    else if (action === "cancel")
      data = await cancelPublicReservation(db, holdingSlug, token, env);
    else data = await recoverBooking(db, holdingSlug, body, env);
    return Response.json(
      canonicalDestinationLinks({ data }),
      {
        headers: {
          "Cache-Control": "private, no-store",
          "Referrer-Policy": "no-referrer",
        },
      },
    );
  } catch (e) {
    return errorResponse(bookingError(e));
  }
}
