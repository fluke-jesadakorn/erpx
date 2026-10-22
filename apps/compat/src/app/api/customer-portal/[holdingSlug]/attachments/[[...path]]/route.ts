// App-owned entry; shared payloads do not import another application.
export { GET, POST, PUT } from "@erpx/feature-portal/routes/api/customer-portal/[holdingSlug]/attachments/[[...path]]/route";
export const dynamic = "force-dynamic";
