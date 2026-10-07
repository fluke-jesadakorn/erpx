import type {z} from 'zod';import {consumptionLotSchema} from './consumption';
export type LotInput=z.infer<typeof consumptionLotSchema>;
export type ConsumptionInput<Effect>={key:string;sourceKind:'maintenance'|'field_service';sourceId:string;sourceVersion:number;branchId:string;departmentId:string|null;
 warehouseId:string;productId:string;quantityMilli:number;entryDate:string;reason:string;ownReservationMilli?:number;allocations:LotInput[];beforeStock?:Effect[]};
