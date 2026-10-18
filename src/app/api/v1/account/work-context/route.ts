import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { NextResponse } from "next/server";
import { z } from "zod";
import { accountWorkContext } from "@/server/modules/identity_access/public/account-work-context";
import { errorResponse } from "@/server/modules/runtime/public/http";
import { holdingBindingBySlug } from "@/server/modules/runtime/public/runtime";

const query = z.object({ from: z.string().min(1).max(2000), companyId: z.string().min(1).max(128).optional() });

export async function GET(request: Request) {
  try {
    const url = new URL(request.url);
    const input = query.parse({ from: url.searchParams.get("from"), companyId: url.searchParams.get("companyId") ?? undefined });
    const data = await accountWorkContext(input);
    const response = NextResponse.json(canonicalDestinationLinks({ data }), { headers: { "Cache-Control": "private, no-store" } });
    // Keep this page's company in this browser even when the new account's
    // saved preference points elsewhere. Selecting a workspace changes it later.
    if (data.current && data.current.kind !== "platform") {
      try {
        const holding = await holdingBindingBySlug(data.current.holdingSlug);
        response.cookies.set(`erpx_company_${holding.id}`, data.current.companyId, {
          httpOnly: true, sameSite: "lax", secure: url.protocol === "https:", path: "/",
          maxAge: 60 * 60 * 24 * 365, priority: "high",
        });
      } catch (error) {
        // An inactive or removed holding still needs the recovery evidence.
        if (!(error instanceof Error) || error.message !== "HOLDING_NOT_FOUND") throw error;
      }
    }
    return response;
  } catch (error) { return errorResponse(error); }
}
