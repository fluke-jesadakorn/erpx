import manifest from "@erpx/platform-workplace/routes/manifest";

export default function appsManifest() {
  return { ...manifest(), name: "ERPX Apps", short_name: "ERPX Apps" };
}

