/** Compatibility entry point; legacy implementation belongs to identity_access. */
export {assertCompanyScope,iamActorFromWork,iamContext,iamPermission} from "@/server/modules/identity_access/adapters/iam-context";
export type {IamActor} from "@/server/modules/identity_access/adapters/iam-context";
