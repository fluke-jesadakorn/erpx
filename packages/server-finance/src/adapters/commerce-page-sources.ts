import "server-only";
export const commerceFinancePageSources={orderPartyJoin:'JOIN finance_parties p ON p.id=o.party_id',parties:(companyId:string,partyKind:string)=>({ sql: "SELECT *,name AS page_key FROM finance_parties WHERE company_id=? AND kind IN ('customer','supplier') AND person_id IS NULL AND (?::text='' OR kind=?)", args: [companyId, partyKind, partyKind] })};
