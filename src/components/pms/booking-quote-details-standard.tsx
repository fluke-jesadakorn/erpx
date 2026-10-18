"use client";
import {FrontLocalizedTree,useFrontLanguage} from "@/components/front/front-language";
import {frontText} from "@/lib/shared/front-language";
import {Table, TableFrame} from "@/components/ui/foundation";
import {money} from "@/server/modules/pms/public/rules/pms-model";
import type {BookingQuote} from "@/server/modules/pms/public/rules/pms-booking";
import {BookingPolicySummary} from "./booking-policy-summary";
import styles from "./guest-booking-management-workspace.module.css";

export function BookingQuoteDetailsStandard({quote}: {quote: BookingQuote}) {
  const language=useFrontLanguage();
  return <FrontLocalizedTree><div className={styles.quote}>
    <TableFrame appearance="standard" label={frontText("Room price and accepted terms",language)}>
      <Table>
        <thead><tr><th>Room / night</th><th>Rooms</th><th>Room charge</th><th>Service</th><th>Tax</th><th>Total</th></tr></thead>
        <tbody>{quote.lines.flatMap((line, index) => line.nights.map(night => <tr key={`${index}-${night.date}`}>
          <td>{line.roomName} · {night.date}</td><td>{line.quantity}</td>
          <td>{money(night.net * line.quantity, quote.currency)}</td><td>{money(night.service * line.quantity, quote.currency)}</td>
          <td>{money(night.tax * line.quantity, quote.currency)}</td><td>{money(night.total * line.quantity, quote.currency)}</td>
        </tr>))}</tbody>
      </Table>
    </TableFrame>
    <p className={styles.total}>Stay total {money(quote.total, quote.currency)}</p>
    <div className={styles.paymentSplit}><p>Pay now <strong>{money(quote.dueNow, quote.currency)}</strong></p><p>Pay at hotel <strong>{money(quote.dueAtHotel, quote.currency)}</strong></p></div>
    {quote.lines.map((line, index) => <div key={index}><span>{line.roomName} · {line.rateName}</span><BookingPolicySummary policy={line.policy}/></div>)}
    <p className={styles.muted}>Availability is checked again when you confirm. Searching does not hold a room.</p>
  </div></FrontLocalizedTree>;
}
