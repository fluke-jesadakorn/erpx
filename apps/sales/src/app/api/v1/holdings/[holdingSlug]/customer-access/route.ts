// App-owned entry; shared payloads do not import another application.
export { nativeCustomerAccessSourcesGet as GET } from "@erpx/feature-customer_authority/lib/access-sources";
export { POST } from "@erpx/feature-sales/routes/api/v1/holdings/[holdingSlug]/customer-access/route";
