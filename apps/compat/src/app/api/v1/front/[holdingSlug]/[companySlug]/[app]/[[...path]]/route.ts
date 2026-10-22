// App-owned entry; shared payloads do not import another application.
export { PUT, GET, POST } from "@erpx/workplace/routes/api/v1/front/[holdingSlug]/[companySlug]/[app]/[[...path]]/route";
export const dynamic = "force-dynamic";
