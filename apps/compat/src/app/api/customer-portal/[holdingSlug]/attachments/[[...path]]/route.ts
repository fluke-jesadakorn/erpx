// App-owned entry; shared payloads do not import another application.
export { GET, POST, PUT } from "@erpx/product-portal-ui/routes/api/customer-portal/[holdingSlug]/attachments/[[...path]]/route";
export const dynamic = "force-dynamic";
