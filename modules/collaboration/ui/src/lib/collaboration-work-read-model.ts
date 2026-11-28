/** UI read models; no server implementation or write authority is imported. */
export type CollaborationRow = Record<string, string | number | boolean | null>;
export type CollaborationCard = CollaborationRow & {
  id: string; version: number; status: string; owner_id: string; due_at: number; owner_eligible: number;
};
export type CollaborationWorkReadModel = {
  relations: CollaborationRow[]; cards: CollaborationCard[]; people: CollaborationRow[];
  sourceTypes: readonly string[]; cardSourceTypes?: readonly string[]; fieldOnlyRelations?:boolean; canWrite: boolean; canReview: boolean;
};
export function validFieldCollaborationReadModel(value: unknown): value is CollaborationWorkReadModel {
  const row = (v: unknown): v is CollaborationRow => !!v && typeof v === 'object' && !Array.isArray(v) && Object.values(v).every(x => x === null || typeof x === 'string' || typeof x === 'boolean' || typeof x === 'number' && Number.isSafeInteger(x));
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const v=value as Record<string,unknown>;
  return Array.isArray(v.sourceTypes)&&v.sourceTypes.length===2&&v.sourceTypes[0]==='field_service_order'&&v.sourceTypes[1]==='support_ticket'&&Array.isArray(v.cardSourceTypes)&&v.cardSourceTypes.length===1&&v.cardSourceTypes[0]==='field_service_order'&&v.fieldOnlyRelations===true&&typeof v.canWrite==='boolean'&&typeof v.canReview==='boolean'
    &&Array.isArray(v.relations)&&v.relations.every(r=>row(r)&&typeof r.id==='string'&&['field_service_order','support_ticket'].includes(String(r.source_type))&&['field_service_order','support_ticket'].includes(String(r.target_type))&&(r.source_type==='field_service_order'||r.target_type==='field_service_order')&&typeof r.source_id==='string'&&typeof r.target_id==='string'&&Number.isSafeInteger(r.version)&&Number(r.version)>0)
    &&Array.isArray(v.cards)&&v.cards.every(r=>row(r)&&typeof r.id==='string'&&r.source_type==='field_service_order'&&typeof r.source_id==='string'&&typeof r.source_title==='string'&&typeof r.source_href==='string'&&r.source_href==='/field-service/'+encodeURIComponent(r.source_id)&&typeof r.owner_id==='string'&&typeof r.title==='string'&&['open','completed','cancelled'].includes(String(r.status))&&Number.isSafeInteger(r.version)&&Number(r.version)>0&&Number.isSafeInteger(r.due_at)&&[0,1].includes(Number(r.owner_eligible)))
    &&Array.isArray(v.people)&&v.people.every(p=>row(p)&&typeof p.id==='string'&&typeof p.name==='string');
}

/** Bind acknowledgement to the original attempt, including an uncertain retry. */
export function matchesCollaborationReceipt(body: string, key: string, value: unknown): boolean {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  let input: Record<string, unknown>;
  try { input = JSON.parse(body); } catch { return false; }
  if (!input || typeof input !== "object" || Array.isArray(input)) return false;
  const relation = input.entity === "relation";
  const changed = input.id !== undefined;
  const action = input.action;
  if (changed && (typeof input.id !== "string" || !input.id || !Number.isSafeInteger(input.expectedVersion) || Number(input.expectedVersion) < 1)) return false;
  const version = changed ? Number(input.expectedVersion) + 1 : 1;
  if (!Number.isSafeInteger(version)) return false;
  let status: string;
  if (!changed) status = relation ? "active" : "open";
  else if (relation && action === "remove") status = "removed";
  else if (!relation && action === "complete") status = "completed";
  else if (!relation && action === "cancel") status = "cancelled";
  else if (!relation && action === "reassign") status = "open";
  else return false;
  const expectedId = changed ? input.id : `${relation ? "record-relation" : "action-card"}-${key}`;
  const receipt = value as Record<string, unknown>;
  return receipt.id === expectedId && receipt.version === version && receipt.status === status;
}

/** Outer-shell read composition. Partitions remain authoritative at each owner;
 * source failures must propagate before calling this pure composition. */
export function mergeCollaborationReadModels(legacy:CollaborationWorkReadModel,native:CollaborationWorkReadModel):CollaborationWorkReadModel{
 if(legacy.relations.some(row=>row.source_type==='field_service_order'||row.target_type==='field_service_order')||legacy.cards.some(row=>row.source_type==='field_service_order'))throw new Error('COLLABORATION_CONSUMER_PARTITION_DRIFT');
 const overlap=(left:CollaborationRow[],right:CollaborationRow[])=>{const ids=new Set(left.map(row=>row.id));return right.some(row=>ids.has(row.id));};
 if(overlap(legacy.relations,native.relations)||overlap(legacy.cards,native.cards))throw new Error('COLLABORATION_CONSUMER_PARTITION_DRIFT');
 const unique=(rows:CollaborationRow[])=>[...new Map(rows.map(row=>[String(row.id),row])).values()];
 return {...legacy,relations:[...legacy.relations,...native.relations],cards:[...legacy.cards,...native.cards],people:unique([...legacy.people,...native.people]),sourceTypes:[...new Set([...legacy.sourceTypes,...native.sourceTypes])],cardSourceTypes:[...new Set([...(legacy.cardSourceTypes??legacy.sourceTypes),...(native.cardSourceTypes??native.sourceTypes)])],fieldOnlyRelations:false,canWrite:legacy.canWrite&&native.canWrite,canReview:legacy.canReview&&native.canReview};
}
