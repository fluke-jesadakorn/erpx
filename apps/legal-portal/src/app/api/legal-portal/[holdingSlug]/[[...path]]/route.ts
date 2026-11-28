// App-owned entry; shared payloads do not import another application.
export { GET, POST } from "@erpx/product-legal-portal-ui/routes/api/legal-portal/[holdingSlug]/[[...path]]/route";
export const dynamic = "force-dynamic";
