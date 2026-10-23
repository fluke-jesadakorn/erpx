// App-owned entry; shared payloads do not import another application.
export { GET, POST } from "@erpx/workplace/routes/api/auth/[...all]/route";
export const dynamic = "force-dynamic";
