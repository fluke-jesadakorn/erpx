import "server-only";
/** Closed owner projections for Platform, Organization and Procurement joins. Original filters, bindings and supplied transactions remain with the caller. */
export const platformOwnerProjectionSources = Object.freeze({
  ecommerce_channels: "SELECT \"id\",\"company_id\",\"name\",\"channel_type\",\"configuration_json\",\"status\",\"created_by\",\"reviewed_by\",\"version\",\"created_at\",\"updated_at\" FROM ecommerce_channels",
  ecommerce_listings: "SELECT \"id\",\"channel_id\",\"product_id\",\"title\",\"currency\",\"price_minor\",\"status\",\"created_by\",\"reviewed_by\",\"version\",\"created_at\",\"updated_at\" FROM ecommerce_listings",
} as const);
