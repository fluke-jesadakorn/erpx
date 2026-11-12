/** Explicit public API. See ownership.json for capability migration status. */
export { customerAccessRequestSchema, customerAccessBinding, customerAccessWorkspace, requestCustomerAccess, changeCustomerAccess, customerAccessHistory, customerAccessWork } from "@erpx/server-customer_authority/public/customer-access";
export { customerDuplicateWorkspace, customerDuplicateDetail, requestCustomerDuplicate, changeCustomerDuplicate, customerDuplicateWork } from "@erpx/server-customer_authority/public/customer-duplicates";
export { customerIdentityWorkspace, customerIdentityDetail, requestCustomerIdentityChange, changeCustomerIdentity, customerIdentityWork } from "@erpx/server-customer_authority/public/customer-identity-changes";
export { portalAttachmentWithdrawProof, requirePortalAttachmentWithdrawCurrent, type PortalAttachmentWithdrawProof } from "@erpx/server-customer_authority/public/attachment-withdraw-proof";
