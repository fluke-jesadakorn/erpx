import type { MetadataRoute } from "next";

export default function manifest(): MetadataRoute.Manifest {
  return {
    name: "ERPX · Work connected",
    short_name: "ERPX",
    description: "A calm, connected workplace for enterprise operations.",
    start_url: "/",
    display: "standalone",
    background_color: "#2F69F5",
    theme_color: "#2F69F5",
    icons: [
      { src: "/icon-192.png", sizes: "192x192", type: "image/png" },
      { src: "/icon-512.png", sizes: "512x512", type: "image/png" },
      {
        src: "/icon-maskable-512.png",
        sizes: "512x512",
        type: "image/png",
        purpose: "maskable",
      },
    ],
  };
}
