// App-owned entry; shared payloads do not import another application.
export { default } from "@erpx/feature-booking/routes/book/[holdingSlug]/[propertyId]/page";
export const dynamic = "force-dynamic";
