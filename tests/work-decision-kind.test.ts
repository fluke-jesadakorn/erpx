import { expect,it } from "vitest";
import { flowWorkActionKind } from "../src/server/flow-work";
import type { FlowView } from "../src/lib/flow";
import { workInView, type WorkItem } from "../src/lib/work-item";

it("uses the owner's flow step for order and payroll approvals, keeping posting and payment separate",()=>{
 const flow=(kind:FlowView["kind"],key:string)=>({kind,current:{key}} as FlowView);
 for(const [kind,key] of [["order","submitted"],["payroll","in_review"],["payroll","payment_draft"]] as const)
   expect(flowWorkActionKind(flow(kind,key))).toBe("approval");
 for(const [kind,key] of [["order","confirmed"],["order","stock_gl"],["invoice","draft"],["invoice","posted"],["journal","draft"],["payroll","approved"],["payroll","payment_approved"],["payroll","payment_exported"]] as const)
   expect(flowWorkActionKind(flow(kind,key))).toBe("work");
 const work=(id:string,actionKind:WorkItem["actionKind"]):WorkItem=>({id,type:"supplier_batch",title:id,detail:"",href:"/finance",status:"ready",updatedAt:1,queue:"todo",canAct:true,relationships:["approval"],actionKind});
 expect(workInView([work("approve","approval"),work("pay","work")],"approvals").map(item=>item.id)).toEqual(["approve"]);
});
