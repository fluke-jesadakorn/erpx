/** Compatibility entry point; legacy implementation belongs to identity_access. */
export {adGuid,assertEmptyUcsContainer,directorySecret,escapeLdap,ldapEnabled,readDirectory,readEntraDirectory,readLdapDirectory,testDirectory,verifyLdapPassword} from "@/server/modules/identity_access/adapters/iam-providers";
export type {ProviderConnection,ProviderSnapshot,ProviderUser} from "@/server/modules/identity_access/adapters/iam-providers";
