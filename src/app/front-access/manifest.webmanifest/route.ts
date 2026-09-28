import { frontAppForHost } from "@/lib/front-host";
export function GET(request: Request) {
  const app = frontAppForHost(request.headers.get("host"));
  if (!app) return new Response("Not found", { status: 404 });
  return Response.json({
    id: "/", name: app === "pms" ? "PMS Front · งานโรงแรม" : "POS Front · งานหน้าร้าน",
    short_name: app === "pms" ? "PMS Front" : "POS Front",
    description: app === "pms" ? "งานหน้าบ้านโรงแรม" : "งานขายหน้าร้าน",
    lang: "th", start_url: "/", scope: "/", display: "standalone",
    background_color: "#f6f8fc", theme_color: app === "pms" ? "#3659c9" : "#137c67",
    icons: [192,512].map((size) => ({ src: `/icon/${size}`, sizes: `${size}x${size}`, type: "image/png", purpose: "any maskable" })),
  }, { headers: { "Content-Type": "application/manifest+json; charset=utf-8", "Cache-Control": "public, max-age=3600" } });
}
