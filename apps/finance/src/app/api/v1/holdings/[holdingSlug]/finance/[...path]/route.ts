// App-owned entry; shared payloads do not import another application.
export { GET, POST } from "@erpx/workplace/routes/api/v1/holdings/[holdingSlug]/finance/[...path]/route";
export const dynamic = "force-dynamic";
