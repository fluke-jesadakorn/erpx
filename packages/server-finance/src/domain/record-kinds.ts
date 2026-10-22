export const financeExtraKinds=['finance_business_contract','finance_business_bill','finance_recognition','finance_obligation','bank_match_group','bank_profile','collection_case','cash_forecast_sources'] as const;
export type FinanceExtraKind=typeof financeExtraKinds[number];
export function isFinanceExtraKind(kind:string):kind is FinanceExtraKind{return financeExtraKinds.some(k=>k===kind);}
export const financeExpansionKinds=['finance_instrument','finance_instrument_run','finance_asset_policy','finance_asset_policy_run'] as const;
export type FinanceExpansionKind=typeof financeExpansionKinds[number];
export function isFinanceExpansionKind(kind:string):kind is FinanceExpansionKind{return financeExpansionKinds.some(k=>k===kind);}
export type FinanceRecordRow=Record<string,string|number|null>;
