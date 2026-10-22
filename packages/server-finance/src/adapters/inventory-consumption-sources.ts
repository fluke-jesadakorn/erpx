import {inventoryReadProjection as inventorycommerceProjection} from '@erpx/server-commerce/public/inventory-read-projections';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';import {assertion} from "@erpx/server-workflow/public/command-assertions";
export function inventoryConsumptionSources(db:SqlDatabase){return {async servicePostingConfig(companyId:string){return db.prepare(`SELECT s.chart_id,c.base_currency currency,s.inventory_account_id,s.cogs_account_id,c.base_currency
  ${inventorycommerceProjection.postingSettings} JOIN finance_charts c ON c.id=s.chart_id AND c.company_id=s.company_id WHERE s.company_id=?`).bind(companyId)
  .first<{chart_id:string;currency:string;inventory_account_id:string;cogs_account_id:string;base_currency:string}>();},
async postingPeriods(companyId:string,entryDate:string,entryDate2:string){return (await db.prepare("SELECT id FROM finance_periods WHERE company_id=? AND state='open' AND start_date<=? AND end_date>=?").bind(companyId,entryDate,entryDate2).all<{id:string}>()).results;},
guardFinanceMapping(key:string,value:"part-finance-mapping",companyId:string,inventory_account_id:string,cogs_account_id:string){return assertion(db,key,value,`EXISTS(SELECT 1 ${inventorycommerceProjection.postingSettings} JOIN finance_charts c ON c.id=s.chart_id
    JOIN finance_accounts inventory ON inventory.id=s.inventory_account_id AND inventory.company_id=s.company_id AND inventory.chart_id=s.chart_id AND inventory.category='asset' AND inventory.status='active'
    JOIN finance_accounts expense ON expense.id=s.cogs_account_id AND expense.company_id=s.company_id AND expense.chart_id=s.chart_id AND expense.category IN ('expense','other_expense') AND expense.status='active'
    WHERE s.company_id=? AND s.inventory_account_id=? AND s.cogs_account_id=? AND inventory.currency=c.base_currency AND expense.currency=c.base_currency AND inventory.id<>expense.id)`,[companyId,inventory_account_id,cogs_account_id]);}};}
