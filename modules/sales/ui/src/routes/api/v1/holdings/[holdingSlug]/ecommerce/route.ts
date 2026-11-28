import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { requestPublicOrigin } from "@erpx/manage-erpx-server/public/public-origin";
import { ApiError, errorResponse } from "@erpx/platform-runtime-server/public/http";
import {
  addCartLine,
  changeEcommerce,
  checkoutCart,
  createCart,
  createChannel,
  ecommerceDetail,
  ecommerceWorkspace,
} from "@erpx/ecommerce-server/public/ecommerce";
import {
  nativeEcommerceListingOrThrow,
  nativeEcommerceListingRequired,
} from "@erpx/sales-ui/public/native-ecommerce-listing";

type Context = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };

export async function GET(request: Request, context: Context) {
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug);
    const id = new URL(request.url).searchParams.get("id");
    return Response.json(
      canonicalDestinationLinks({
        data: id
          ? await ecommerceDetail(actor, id)
          : await ecommerceWorkspace(actor),
      }),
      { headers },
    );
  } catch (error) {
    return errorResponse(error);
  }
}

export async function POST(request: Request, context: Context) {
  try {
    if (
      request.headers.get("origin") &&
      request.headers.get("origin") !== requestPublicOrigin(request)
    )
      throw new ApiError(
        403,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
      );
    const key = request.headers.get("Idempotency-Key");
    if (!key)
      throw new ApiError(
        422,
        "COMMAND_KEY_REQUIRED",
        "Idempotency-Key is required.",
      );
    const holdingSlug = (await context.params).holdingSlug;
    const actor = await requireWorkContext(holdingSlug);
    const input = await request.json();
    // Native fence: listing draft create, draft retitle and review decisions
    // are owner-complete in Rust. The retained createListing/changeEcommerce
    // writers for this entity are gone, so an absent native transport fails
    // closed (503) instead of reaching a TypeScript fallback.
    const native = await nativeEcommerceListingOrThrow(request, actor, holdingSlug, input, key);
    if (native) return Response.json(canonicalDestinationLinks({ data: native }), { headers });
    if (nativeEcommerceListingRequired(input))
      throw new ApiError(
        503,
        "NATIVE_ECOMMERCE_LISTING_UNAVAILABLE",
        "The native Ecommerce listing service is unavailable.",
      );
    const data =
      typeof input.id === "string"
        ? await changeEcommerce(actor, input.entity, input.id, input, key)
        : input.entity === "channel"
          ? await createChannel(actor, input, key)
          : input.entity === "cart"
            ? await createCart(actor, input, key)
            : input.entity === "cart-line"
              ? await addCartLine(actor, input, key)
              : input.entity === "checkout"
                ? await checkoutCart(actor, input, key)
                : (() => {
                    throw new ApiError(
                      422,
                      "ECOMMERCE_ENTITY_REQUIRED",
                      "Choose an E-commerce command.",
                    );
                  })();
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (error) {
    return errorResponse(error);
  }
}
