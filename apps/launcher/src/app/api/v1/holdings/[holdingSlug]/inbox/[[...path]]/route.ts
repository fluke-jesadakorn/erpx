// App-owned entry; shared payloads do not import another application.
export { GET, POST } from "@erpx/feature-apps/routes/api/v1/holdings/[holdingSlug]/inbox/[[...path]]/route";
export const dynamic = "force-dynamic";
