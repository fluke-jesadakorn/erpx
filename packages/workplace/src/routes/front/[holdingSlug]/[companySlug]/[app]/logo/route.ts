import { frontCompanyBrand, storefrontLogo } from "@erpx/server-work_projection/public/front-app";

export async function GET(
  _request: Request,
  { params }: { params: Promise<{ holdingSlug: string; companySlug: string }> },
) {
  const { holdingSlug, companySlug } = await params;
  try {
    const entry = await frontCompanyBrand(holdingSlug, companySlug);
    if (!entry) return new Response("Not found", { status: 404 });
    const logo = await storefrontLogo(holdingSlug, companySlug);
    if (!logo) return new Response("Not found", { status: 404 });
    return new Response(logo.object.body, {
      headers: {
        "Content-Type": logo.mediaType,
        "Cache-Control": "private, no-store",
        "X-Content-Type-Options": "nosniff",
      },
    });
  } catch (error) { throw error; }
}
