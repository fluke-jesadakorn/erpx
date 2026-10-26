// App-owned entry; shared payloads do not import another application.
export { GET, POST, DELETE } from "@erpx/workplace/routes/api/v1/holdings/[holdingSlug]/push/route";
export const dynamic = "force-dynamic";
