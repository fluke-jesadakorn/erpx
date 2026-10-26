import manifest from "@erpx/workplace/routes/manifest";

export default function appsManifest() {
  return { ...manifest(), name: "ERPX Apps", short_name: "ERPX Apps" };
}

