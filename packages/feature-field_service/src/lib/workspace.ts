/** Serializable data consumed by the Service workspace and its existing forms. */
export type FieldRow = Record<string,string|number|null>;
export type ServiceAgreement = FieldRow & {id:string;version:number;status:string;created_by:string;updated_at:number;can_review:number;warranty_discount_bps:number};
export type ServiceOrder = FieldRow & {id:string;version:number;status:string;technician_id:string;created_by:string;updated_at:number;scheduled_at:number|null;technician_eligible:number;can_review:number;issue:string;site:string;exception_reason:string|null};
export type FieldWorkspace = {
 agreements:ServiceAgreement[];orders:ServiceOrder[];
 customers:{id:string;name:string;currency:string}[];
 assets:{id:string;code:string;name:string;version:number;branch_id:string;department_id:string|null}[];
 tickets:FieldRow[];people:{id:string;name:string;branch_id:string;department_id:string|null}[];
 nodes:{id:string;kind:'branch'|'headquarters'|'department';display_name:string;branch_id:string}[];
 referenceScope:{branchId:string|null;departmentId:string|null};workspaceKey:string;
 canWrite:boolean;canCreateAgreement:boolean;canCreateOrder:boolean;canUseParts:boolean;canUseAssetReferences:boolean;
};
