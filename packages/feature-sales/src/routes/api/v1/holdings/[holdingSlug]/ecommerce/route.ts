import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import {
  addCartLine,
  changeEcommerce,
  checkoutCart,
  createCart,
  createChannel,
  createListing,
  ecommerceDetail,
  ecommerceWorkspace,
} from "@erpx/server-ecommerce/public/ecommerce";

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
    const actor = await requireWorkContext((await context.params).holdingSlug);
    const input = await request.json();
    const data =
      typeof input.id === "string"
        ? await changeEcommerce(actor, input.entity, input.id, input, key)
        : input.entity === "channel"
          ? await createChannel(actor, input, key)
          : input.entity === "listing"
            ? await createListing(actor, input, key)
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
