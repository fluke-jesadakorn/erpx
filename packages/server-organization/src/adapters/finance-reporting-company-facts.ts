/** Closed Company projection composed into Finance's original single scoped statement. */
export function financeReportingCompanyFacts(){return {
 companyJoin:"JOIN organization_nodes n ON n.id=c.company_id",
 companyName:"n.display_name company_name",
} as const;}
