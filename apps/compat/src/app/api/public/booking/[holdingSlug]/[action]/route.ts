// App-owned entry; shared payloads do not import another application.
export { GET, POST } from "@erpx/feature-booking/routes/api/public/booking/[holdingSlug]/[action]/route";
export const dynamic = "force-dynamic";
