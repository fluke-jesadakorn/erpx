// App-owned entry; shared payloads do not import another application.
export { GET } from "@erpx/product-people-ui/routes/api/v1/holdings/[holdingSlug]/people/access/[[...path]]/route";
export const dynamic = "force-dynamic";
