"use client";
import { useExpenseCommand } from "./expense-controls";
export function ExpensePosting({base,id,version,periodId,entryDate,canPost}:{base:string;id:string;version:number;periodId:string;entryDate:string;canPost:boolean}) {
  const {send,pending,error} = useExpenseCommand(base);
  return <div>{error && <p role="alert" className="error">{error}</p>}{canPost && <button className="button" disabled={pending || !periodId || !entryDate} onClick={() => void send(`/${id}/post`,{expectedVersion:version,periodId,entryDate})}>Confirm Debit / Credit and post to GL</button>}</div>;
}
