import type {WorkContext} from '../../identity_access/public/authorization';import type {Row} from '../../commerce/public/commerce';import type {ManufacturingMaterialAllocation,ManufacturingOutputAllocation} from '../domain/lots';import {materialAllocationViolation,materialLotViolation,outputAllocationViolation} from '../domain/lot-policy';import {quantityMilli,scaledAmount} from '@/lib/commerce-math';import {BusinessRuleError,type Clock} from '../../shared';export interface ManufacturingLotRepository<Effect>{materialLot(lotId:string,companyId:string,productId:string,warehouseId:string):Promise<Row|null>;
lockMaterialLot(lotId:string,warehouseId:string):Effect;
guardMaterialLot(key:string,value:string,lotId:string,warehouseId:string,balanceVersion:number,quantity:number,value2:number,entryDate:string):Effect;
prepareLotConsumption(quantity:number,value:number,lotId:string,warehouseId:string):Effect;
prepareLotDepletion(lotId:string,lotId2:string):Effect;
prepareMaterialLotIssue(value:string,orderId:string,orderVersion:number,productId:string,lotId:string,quantity:number,value2:number,userId:string,now:number):Effect;
prepareMaterialLotMove(value:string,lotId:string,warehouseId:string,value2:string,value3:number,value4:number,entryDate:string,userId:string,now:number):Effect;
materialLotIssues(orderId:string):Promise<{results:Row[]}>;
prepareOutputLot(lotId:string,companyId:string,productId:string,trackingMode:string,value:string | null,value2:string | null,entryDate:string,value3:string | null,userId:string,now:number):Effect;
prepareOutputLotStock(lotId:string,warehouseId:string,quantity:number,value:number):Effect;
prepareOutputEvidence(outputId:string,orderId:string,orderVersion:number,lotId:string,quantity:number,value:number,userId:string,now:number):Effect;
prepareOutputLotMove(value:string,lotId:string,warehouseId:string,value2:string,quantity:number,value3:number,entryDate:string,userId:string,now:number):Effect;
prepareGenealogy(value:string,orderId:string,id:string | number | null,outputId:string,inputQty:number,orderVersion:number,now:number):Effect;}export interface ManufacturingLotPorts<Effect>{repository:ManufacturingLotRepository<Effect>;clock:Clock;}export function createManufacturingLotApplication<Effect>(ports:ManufacturingLotPorts<Effect>){const r=ports.repository;async function prepareManufacturingMaterialLots( a: WorkContext, input: {
  key: string; orderId: string; orderVersion: number; warehouseId: string; productId: string; trackingMode: string;
  requiredMilli: number; entryDate: string; allocations: ManufacturingMaterialAllocation[];
}) {
  const allocations = input.allocations.filter(x => x.productId === input.productId);
  const allocationViolation = materialAllocationViolation(input.trackingMode, input.requiredMilli, allocations, quantityMilli);
  if (allocationViolation) throw new BusinessRuleError(allocationViolation.status, allocationViolation.code, allocationViolation.message);
  if (input.trackingMode === "none") return null;
  let valueMinor = 0;
  const steps: Effect[] = [];
  for (const [n, allocation] of allocations.entries()) {
    const quantity = quantityMilli(allocation.quantity), lot = await r.materialLot(allocation.lotId,a.companyId,input.productId,input.warehouseId);
    const lotViolation = materialLotViolation(lot, { balanceVersion: allocation.balanceVersion, quantity, trackingMode: input.trackingMode, entryDate: input.entryDate });
    if (lotViolation) throw new BusinessRuleError(lotViolation.status, lotViolation.code, lotViolation.message);
    const value = quantity === Number(lot!.quantity_milli) ? Number(lot!.value_minor) : scaledAmount(Number(lot!.value_minor), quantity, Number(lot!.quantity_milli)), now = ports.clock.now();
    valueMinor += value;
    steps.push(r.lockMaterialLot(allocation.lotId,input.warehouseId),
      r.guardMaterialLot(input.key,`material-lot-${input.productId}-${n}`,allocation.lotId,input.warehouseId,allocation.balanceVersion,quantity,value,input.entryDate),
      r.prepareLotConsumption(quantity,value,allocation.lotId,input.warehouseId),
      r.prepareLotDepletion(allocation.lotId,allocation.lotId),
      r.prepareMaterialLotIssue(`mfg-input-lot-${input.key}-${input.productId}-${n}`,input.orderId,input.orderVersion,input.productId,allocation.lotId,quantity,value,a.userId,now),
      r.prepareMaterialLotMove(`mfg-input-move-${input.key}-${input.productId}-${n}`,allocation.lotId,input.warehouseId,`${input.orderId}:v${input.orderVersion}:${input.productId}`,-quantity,-value,input.entryDate,a.userId,now),
    );

  }
  return { steps, valueMinor };
}
async function prepareManufacturingOutputLots( a: WorkContext, input: {
  key: string; orderId: string; orderVersion: number; warehouseId: string; productId: string; trackingMode: string;
  outputMilli: number; totalMinor: number; entryDate: string; allocations: ManufacturingOutputAllocation[];
}) {
  const allocationViolation = outputAllocationViolation(input.trackingMode, input.outputMilli, input.allocations, quantityMilli);
  if (allocationViolation) throw new BusinessRuleError(allocationViolation.status, allocationViolation.code, allocationViolation.message);
  if (input.trackingMode === "none") return [];
  const issues = (await r.materialLotIssues(input.orderId)).results;
  const steps: Effect[] = [], priorInput = new Map<string, number>();
  let cumulativeQty = 0, cumulativeValue = 0;
  for (const [n, allocation] of input.allocations.entries()) {
    const quantity = quantityMilli(allocation.quantity);
    if (input.trackingMode === "serial" && quantity !== 1000) throw new BusinessRuleError(422, "SERIAL_QUANTITY_INVALID", "Each output serial must identify one unit.");
    if (allocation.expiryDate && allocation.expiryDate < input.entryDate) throw new BusinessRuleError(422, "MANUFACTURING_OUTPUT_EXPIRY", "New output cannot already be expired.");
    cumulativeQty += quantity;
    const cumulative = n === input.allocations.length - 1 ? input.totalMinor : scaledAmount(input.totalMinor, cumulativeQty, input.outputMilli),
      value = cumulative - cumulativeValue, lotId = `mfg-output-lot-${input.key}-${n}`, outputId = `mfg-output-${input.key}-${n}`, now = ports.clock.now();
    cumulativeValue = cumulative;
    steps.push(r.prepareOutputLot(lotId,a.companyId,input.productId,input.trackingMode,allocation.lotCode ?? null,input.trackingMode === "serial" ? allocation.serialNumber! : null,input.entryDate,allocation.expiryDate ?? null,a.userId,now),
      r.prepareOutputLotStock(lotId,input.warehouseId,quantity,value),
      r.prepareOutputEvidence(outputId,input.orderId,input.orderVersion,lotId,quantity,value,a.userId,now),
      r.prepareOutputLotMove(`mfg-output-move-${input.key}-${n}`,lotId,input.warehouseId,`${input.orderId}:v${input.orderVersion}`,quantity,value,input.entryDate,a.userId,now),
    );
    for (const [issueIndex, issue] of issues.entries()) {
      const cumulativeInput = n === input.allocations.length - 1 ? Number(issue.quantity_milli) : scaledAmount(Number(issue.quantity_milli), cumulativeQty, input.outputMilli),
        inputQty = cumulativeInput - (priorInput.get(String(issue.id)) ?? 0);
      priorInput.set(String(issue.id), cumulativeInput);
      steps.push(r.prepareGenealogy(`mfg-lot-genealogy-${input.key}-${n}-${issueIndex}`,input.orderId,issue.id,outputId,inputQty,input.orderVersion,now));
    }
  }
  return steps;
}return {prepareManufacturingMaterialLots,prepareManufacturingOutputLots};}
