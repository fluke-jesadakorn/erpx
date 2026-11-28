// App-owned entry; shared payloads do not import another application.
export { GET, POST } from "@erpx/legal-ui/routes/api/v1/holdings/[holdingSlug]/legal/[[...path]]/route";
export const dynamic = "force-dynamic";
