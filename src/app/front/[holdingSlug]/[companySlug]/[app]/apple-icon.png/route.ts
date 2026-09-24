import { frontCompanyBrand, storefrontLogo } from "@/server/front-app";
import { storefrontAppIconPng } from "@/server/front-icon";

export const dynamic = "force-dynamic";

export async function GET(
  _request: Request,
  { params }: { params: Promise<{ holdingSlug: string; companySlug: string; app: string }> },
) {
  const { holdingSlug, companySlug, app } = await params;
  if (app !== "pos" && app !== "pms") return new Response("Not found", { status: 404 });
  try {
    const entry = await frontCompanyBrand(holdingSlug, companySlug);
    if (!entry) return new Response("Not found", { status: 404 });
    const logo = await storefrontLogo(holdingSlug, companySlug);
    const bytes = logo ? new Uint8Array(await logo.object.arrayBuffer()) : undefined;
    const png = await storefrontAppIconPng({ shortName: entry.shortName, accentColor: entry.accentColor, app, size: 180, logo: bytes });
    return new Response(png, {
      headers: { "Content-Type": "image/png", "Cache-Control": "private, no-store", "X-Content-Type-Options": "nosniff" },
    });
  } catch (error) { throw error; }
}
