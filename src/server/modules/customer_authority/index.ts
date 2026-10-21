/** Explicit public API. See ownership.json for capability migration status. */
export { customerAccessRequestSchema, customerAccessBinding, customerAccessWorkspace, requestCustomerAccess, changeCustomerAccess, customerAccessHistory, customerAccessWork, issueCustomerPortalInvitation, customerPortalInvitationHistory } from "./public/customer-access";
export { customerDuplicateWorkspace, customerDuplicateDetail, requestCustomerDuplicate, changeCustomerDuplicate, customerDuplicateWork } from "./public/customer-duplicates";
export { customerIdentityWorkspace, customerIdentityDetail, requestCustomerIdentityChange, changeCustomerIdentity, customerIdentityWork } from "./public/customer-identity-changes";
export { drainCustomerPortalInvitations } from "./public/portal-invitation-delivery";
export { customerPortalInvitationEnrollment, releaseCustomerPortalEnrollmentAttempt, acceptCustomerPortalInvitation } from "./public/portal-invitation-enrollment";
