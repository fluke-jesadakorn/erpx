// App-owned entry; shared payloads do not import another application.
export { GET, POST, PATCH } from "@erpx/workplace/routes/api/v1/holdings/[holdingSlug]/overtime-requests/[[...path]]/route";
export const dynamic="force-dynamic";
