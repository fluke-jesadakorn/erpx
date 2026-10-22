// App-owned entry; shared payloads do not import another application.
export { GET } from "@erpx/feature-finance/routes/api/v1/holdings/[holdingSlug]/finance/route";
export const dynamic = "force-dynamic";
