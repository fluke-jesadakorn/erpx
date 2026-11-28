export type HospitalityDeliveryGates = {
    automated: boolean;
    browser: boolean;
    provider: boolean;
    production: boolean;
};
export function productionAccepted(gates: HospitalityDeliveryGates | undefined) { return Boolean(gates?.automated && gates.browser && gates.provider && gates.production); }
