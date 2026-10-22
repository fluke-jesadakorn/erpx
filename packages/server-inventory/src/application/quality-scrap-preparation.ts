import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
type Row=Record<string,string|number|null>;
export type QualityScrapCount={key:string;id:string;now:number;countId:string;lineId:string;number:string;input:{dispositionDate:string;reason:string};h:Row;stock:Row;scrapValue:number;countedQuantity:number;countedValue:number;lotAdjustments:{lot:Row;reduce:number;lotValue:number}[]};

export interface QualityScrapCountPreparation<Effect>{
  count(actor:WorkContext,data:QualityScrapCount):Effect;
  line(data:QualityScrapCount):Effect;
  lotGuard(key:string,index:number,lot:Row,warehouse:Row[string]):Effect;
  lotLine(id:string,lineId:string,lot:Row,countedQuantity:number,countedValue:number):Effect;
  submit(countId:string):Effect;
}
export function createQualityScrapCountPreparation<Effect>(repository:QualityScrapCountPreparation<Effect>){
  return {
    prepare(actor:WorkContext,data:QualityScrapCount):Effect[]{
      const steps=[repository.count(actor,data),repository.line(data)];
      for(const [index,{lot,reduce,lotValue}] of data.lotAdjustments.entries()){
        steps.push(repository.lotGuard(data.key,index,lot,data.h.warehouse_id),repository.lotLine(`${data.lineId}-lot-${index}`,data.lineId,lot,Number(lot.quantity_milli)-reduce,Number(lot.value_minor)-lotValue));
      }
      return steps;
    },
    submit:repository.submit,
  };
}
