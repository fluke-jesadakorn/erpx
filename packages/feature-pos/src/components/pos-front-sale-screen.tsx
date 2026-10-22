"use client";
import dynamic from "next/dynamic";
import { FrontLocalizedTree } from "@erpx/front/components/front-language";
import type {FrontPosDailyContext} from "@erpx/feature-pos/components/pos-front-console";


const PosSaleWorkspace = dynamic(() => import("@erpx/feature-pos/components/standard-pos-sale-workspace").then(module => module.StandardPosSaleWorkspace));
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
