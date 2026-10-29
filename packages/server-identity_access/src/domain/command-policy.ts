export const iamCommandKeyValid = (key: string) => /^[a-zA-Z0-9:_.-]{8,180}$/.test(key);
export const iamCommandNeedsFreshMfa = (operation: string) => /^(people\.(access|relationships)\.apply|role\.propose|assignment\.propose|migration\.prepare|mapping\.propose|connection\.activate\.propose|review\.decide|emergency\.|identity\.restore\.propose|invitation\.propose|beta\.invite)/.test(operation);
export type IamCommandAssignment = {
    role: string;
    version: string | number;
};
export type IamCommandRolePolicy = {
    published_revision: string | number;
    grantable_json: string[];
};
export type IamCommandMember = {
    status: string;
    version: string | number;
    login_policy: string;
    identity_id: string;
};
export type IamCommandDirectoryIdentity = {
    enabled: boolean;
    observed_at: string | number;
    status: string;
};
export type IamCommandWorkContext = {
    revision: string | number;
    assignment_id: string;
    assignment_version: string | number;
};
export type IamCommandReceipt = {
    request_hash: string;
    result: unknown;
};
