// App-owned entry; shared payloads do not import another application.
export { GET, POST } from "@erpx/feature-legal-portal/routes/api/legal-portal/[holdingSlug]/[[...path]]/route";
export const dynamic = "force-dynamic";
