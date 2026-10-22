// App-owned entry; shared payloads do not import another application.
export { GET, POST } from "@erpx/feature-pms/routes/api/v1/holdings/[holdingSlug]/pms/imports/route";
export const dynamic="force-dynamic";
