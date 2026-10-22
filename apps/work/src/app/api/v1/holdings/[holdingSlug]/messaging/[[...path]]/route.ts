// App-owned entry; shared payloads do not import another application.
export { GET, POST, PUT } from "@erpx/feature-work/routes/api/v1/holdings/[holdingSlug]/messaging/[[...path]]/route";
export const dynamic = "force-dynamic";
