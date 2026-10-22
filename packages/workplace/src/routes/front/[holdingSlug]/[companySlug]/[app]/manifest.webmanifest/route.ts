import type { MetadataRoute } from "next";
import { frontCompanyBrand, type FrontAppKind } from "@erpx/server-work_projection/public/front-app";

export const dynamic = "force-dynamic";

export async function GET(
  _request: Request,
  { params }: { params: Promise<{ holdingSlug: string; companySlug: string; app: string }> },
) {
  const { holdingSlug, companySlug, app } = await params;
  if (app !== "pos" && app !== "pms") return new Response("Not found", { status: 404 });
  const entry = await frontCompanyBrand(holdingSlug, companySlug);
  if (!entry) return new Response("Not found", { status: 404 });
  const kind = app as FrontAppKind;
  const base = "/front/" + encodeURIComponent(holdingSlug) + "/" + encodeURIComponent(companySlug) + "/" + kind + "/";
  const title = entry.shortName + (kind === "pos" ? " POS" : " PMS");
  const manifest: MetadataRoute.Manifest = {
    id: base,
    name: title,
    short_name: Array.from(title).slice(0, 24).join(""),
    description: entry.companyName + (kind === "pos" ? " point of sale" : " property management"),
    lang: "th",
    start_url: base,
    scope: base,
    display: "standalone",
    background_color: "#f4f6fa",
    theme_color: entry.accentColor,
    icons: [
      { src: base + "icon/192", sizes: "192x192", type: "image/png", purpose: "any" },
      { src: base + "icon/512", sizes: "512x512", type: "image/png", purpose: "any" },
      { src: base + "icon/512", sizes: "512x512", type: "image/png", purpose: "maskable" },
    ],
  };
  return Response.json(manifest, {
    headers: { "Content-Type": "application/manifest+json; charset=utf-8", "Cache-Control": "private, no-store" },
  });
}
