/** Exact original native POS rollback outcomes only. These are thrown inside
 * posCommand's owned transaction (before its receipt) and become HTTP errors
 * after rollback. A previous unknown must never be released by this table. */
export function isFirstPosWorkspaceRollback(operation:string,status:number,code:unknown){
 if(typeof code!=='string')return false;
 const invalid:Record<string,readonly string[]>={
  'sale.add':['POS_PRODUCT_INVALID','POS_MODIFIER_INVALID','POS_RESTAURANT_REQUIRED'],
  'refund.request':['POS_DUPLICATE_LINE','POS_ADVANCE_REFUND','POS_REFUND_QUANTITY','POS_REFUND_EMPTY'],
  'refund.reject':['POS_REASON_REQUIRED'],
  'refund.approve':['POS_REFUND_PROOF_REQUIRED','POS_RETURN_NOT_STOCK'],
  'day.prepare':['POS_FUTURE_DATE','POS_DAY_EMPTY'],
  'accounting.prepare':['POS_ACCOUNT_INVALID','POS_CURRENCY_MISMATCH'],
  'settings.save':['POS_ACCOUNT_INVALID','POS_CURRENCY_MISMATCH'],
  'table.create':['POS_TABLE_NAME_EXISTS'],
  'catalog.save':['POS_PRODUCT_INVALID','POS_RECIPE_REQUIRED','POS_MODIFIER_DUPLICATE','POS_INGREDIENT_INVALID','POS_RESTAURANT_REQUIRED'],
  'payment.create':['POS_PAYMENT_AMOUNT','POS_REFERENCE_REQUIRED','POS_PROMPTPAY_EMAIL_REQUIRED'],
  'cash.create':['POS_CASH_INVALID'],
  'shift.close':['POS_SHIFT_INVALID','POS_CASH_COUNT'],
  'customer.create':['POS_CUSTOMER_DUPLICATE'],
 };
 if(status===422&&invalid[operation]?.includes(code))return true;
 if(status!==409)return false;
 // Native old-receipt lookup precedes perform. Each selected version failure
 // below aborts that same transaction before the new receipt can be committed.
 if(code==='POS_VERSION_CONFLICT'&&['accounting.prepare','accounting.approve','settings.save','settings.operations','register.save','catalog.image','catalog.save','shift.close','shift.review','cash.create','cash.approve','sale.add','sale.remove','sale.update_line','sale.customer','sale.discount','sale.move_table','sale.cancel','sale.hold','sale.resume','sale.merge','sale.split','sale.approve_discount','sale.complete','sale.charge_to_room','payment.create','payment.verify_transfer','payment.cancel_transfer','payment.retry','payment.cancel','kitchen.send','kitchen.start','kitchen.ready','kitchen.deliver','kitchen.cancel','refund.request','refund.approve','refund.reject','day.confirm','settlement.refresh','settlement.map_external','settlement.match'].includes(operation))return true;
 if(operation==='sale.add'&&code==='POS_CATALOG_CHANGED')return true;
 if(operation==='day.prepare'&&code==='POS_DAY_BLOCKED'||operation==='day.confirm'&&code==='POS_DAY_CHANGED')return true;
 return ['sale.complete','sale.charge_to_room','kitchen.send','kitchen.start'].includes(operation)&&code==='POS_STOCK_UNAVAILABLE';
}
