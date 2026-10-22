// App-owned entry; shared payloads do not import another application.
export { GET, DELETE, PATCH } from "@erpx/workplace/routes/api/v1/holdings/[holdingSlug]/records/[module]/[id]/route";
export const dynamic = "force-dynamic";
