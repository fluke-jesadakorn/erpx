// Frontend interaction identity only. All Field mutations and confirmation
// use native-reserve-command; there is no generic TS command implementation.
export type FieldServiceSource={base:string;workspaceKey:string;recordId:string|null;targets:Record<string,{version:number;status:string;customerId?:string;agreementId?:string;technicianId?:string;scheduledAt?:number|null}>};
export type FieldServiceReceipt={id:string;status:string;version:number;ticketId?:string};
export const fieldServiceOwner=(source:FieldServiceSource)=>JSON.stringify([source.base,source.workspaceKey,source.recordId]);
