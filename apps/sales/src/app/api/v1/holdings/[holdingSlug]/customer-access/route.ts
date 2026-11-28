// App-owned entry; shared payloads do not import another application.
export { nativeCustomerAccessSourcesGet as GET } from "@erpx/customer-authority-ui/lib/access-sources";
export { POST } from "@erpx/sales-ui/routes/api/v1/holdings/[holdingSlug]/customer-access/route";
