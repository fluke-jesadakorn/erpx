export type QualityRow=Record<string,string|number|null>;
export type ReceiptQualityHold={id:string;key:string;branchId:string;warehouseId:string;productId:string;lotId?:string;quantityMilli:number;reason:string;dueDate:string;sourceId:string;sourceType?:'goods_receipt'|'customer_return'};
