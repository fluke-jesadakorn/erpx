import { frontCompanyBrand, storefrontLogo } from "@erpx/server-work_projection/public/front-app";
import { storefrontAppIconPng } from "@erpx/server-work_projection/public/front-icon";

export async function GET(
  _request: Request,
  { params }: { params: Promise<{ holdingSlug: string; companySlug: string; app: string; size: string }> },
) {
  const { holdingSlug, companySlug, app, size: rawSize } = await params;
  if ((app !== "pos" && app !== "pms") || !["192", "512"].includes(rawSize))
    return new Response("Not found", { status: 404 });
  try {
    const entry = await frontCompanyBrand(holdingSlug, companySlug);
    if (!entry) return new Response("Not found", { status: 404 });
    const size = Number(rawSize);
    const logo = await storefrontLogo(holdingSlug, companySlug);
    const logoBytes = logo ? new Uint8Array(await logo.object.arrayBuffer()) : undefined;
    const png = await storefrontAppIconPng({ shortName: entry.shortName, accentColor: entry.accentColor, app, size, logo: logoBytes });
    return new Response(new Uint8Array(png), {
      headers: { "Content-Type": "image/png", "Cache-Control": "private, no-store", "X-Content-Type-Options": "nosniff" },
    });
  } catch (error) { throw error; }
}
