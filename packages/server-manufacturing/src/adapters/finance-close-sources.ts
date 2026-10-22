export const financeWorkspaceOpenManufacturingSql="(SELECT count(*) FROM manufacturing_orders o WHERE o.company_id=p.company_id AND o.status IN ('reserved','in_progress','awaiting_qc'))";
