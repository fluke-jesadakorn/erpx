import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { nativeInventoryStockRegisterFromHeaders } from "@erpx/feature-inventory/lib/native-availability";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { Card, Table, TableFrame } from "@erpx/ui/components/foundation";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";

export default async function Page({params}:{params:Promise<{holdingSlug:string}>}) {
  const {holdingSlug}=await params, actor=await requireWorkContext(holdingSlug);
  const nativeActor=actor.authority&&actor.iam?{
    userId:actor.userId,holdingId:actor.holdingId,holdingSlug,companyId:actor.companyId,
    branchId:actor.branchId,departmentId:actor.departmentId,assignmentId:actor.assignmentId,
    revision:actor.authority.revision,assignmentVersion:actor.authority.assignmentVersion,
    branchSlug:actor.scope.kind==="holding"?undefined:actor.scope.branchSlug??undefined,
    departmentSlug:actor.scope.kind==="department"?actor.scope.departmentSlug??undefined:undefined,
  }:undefined;
  const register=await nativeInventoryStockRegisterFromHeaders(nativeActor,holdingSlug).catch(error=>pageError(error,holdingSlug));
  if(!register) return <main><h1>Stock register unavailable</h1></main>;
  return <main style={{padding:"2rem"}}>
    <h1>Native stock register</h1>
    <p>{register.currency} · tracked {register.totals.tracked_quantity_milli/1000} · lots {register.totals.lot_quantity_milli/1000} · consistent {String(register.totals.consistent)}</p>
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>Stock</h2>
      <TableFrame appearance="standard" label="Stock"><Table className="business-table">
        <thead><tr><th>Product</th><th>Warehouse</th><th>Quantity</th><th>Available</th><th>Value</th></tr></thead>
        <tbody>{register.stock.map(row=><tr key={`${row.warehouse_id}:${row.product_id}`}><td>{row.sku} · {row.name}</td><td>{row.warehouse_name}</td><td>{row.quantity_milli/1000}</td><td>{row.available_milli/1000}</td><td>{minorToDecimal(row.value_minor)} {register.currency}</td></tr>)}</tbody>
      </Table></TableFrame>
    </Card>
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>Lots</h2>
      {register.lots.length===0?<p>No tracked lots.</p>:<ul>{register.lots.map(lot=><li key={lot.id}>{lot.lot_code??lot.serial_number} · {lot.sku} · {lot.status} · held {lot.quality_hold_milli/1000}</li>)}</ul>}
    </Card>
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>Counts</h2>
      {register.counts.length===0?<p>No counts.</p>:<ul>{register.counts.map(count=><li key={count.id}>{count.number} · {count.status} · net {minorToDecimal(count.net_value_minor)}</li>)}</ul>}
    </Card>
  </main>;
}
