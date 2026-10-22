// App-owned entry; shared payloads do not import another application.
export { GET, POST } from "@erpx/feature-account/routes/api/v1/account/iam/[[...path]]/route";
export const dynamic="force-dynamic";
