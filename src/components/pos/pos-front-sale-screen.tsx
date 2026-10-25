"use client";
import dynamic from "next/dynamic";
import { FrontLocalizedTree } from "@/components/front/front-language";
import type {FrontPosDailyContext} from "./pos-front-console";


const PosSaleWorkspace = dynamic(() => import("./pos-sales").then(module => module.PosSaleWorkspace));
export function FrontPosDailySaleScreen({context}: {context: FrontPosDailyContext}) {
  const {data, api, erpBase, busy, send, activeRegisterId, setRegisterId, setSelected, pick, setTab} = context;
  return <FrontLocalizedTree>{(
              <PosSaleWorkspace
                data={data}
                api={api}
                erpBase={erpBase}
                busy={busy}
                send={send}
                registerId={activeRegisterId}
                setRegisterId={(id) => {
                  setRegisterId(id);
                  setSelected("");
                }}
                pickSale={pick}
                openShifts={() => setTab("shifts")}
              />
            )}</FrontLocalizedTree>;
}
