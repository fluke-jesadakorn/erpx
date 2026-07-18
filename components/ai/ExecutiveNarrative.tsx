'use client';

import React from 'react';
import { AiActionButton } from './AiActionButton';

interface ExecutiveNarrativeProps {
  execReport: any;
  audience: 'cfo' | 'ceo';
}

export const ExecutiveNarrative: React.FC<ExecutiveNarrativeProps> = ({ execReport, audience }) => {
  if (!execReport) return null;

  const k = execReport.kpis || {};
  const cf = execReport.cashflow || {};
  const tb = execReport.trialBalance || { debit: 0, credit: 0, isBalanced: true };

  const summary = [
    `Cash position: ${k.totalCash?.toLocaleString('th-TH', { maximumFractionDigits: 0 })} THB`,
    `MTD expenses: ${k.mtdExpenses?.toLocaleString('th-TH', { maximumFractionDigits: 0 })} THB`,
    `Outstanding liabilities: ${k.outstandingLiabilities?.toLocaleString('th-TH', { maximumFractionDigits: 0 })} THB`,
    `Net cash flow (MTD): ${cf.netCashFlow?.toLocaleString('th-TH', { maximumFractionDigits: 0 })} THB`,
    `Trial balance: ${tb.isBalanced ? 'balanced' : 'MISMATCH'} (Dr ${tb.debit?.toLocaleString('th-TH', { maximumFractionDigits: 0 })} / Cr ${tb.credit?.toLocaleString('th-TH', { maximumFractionDigits: 0 })})`,
  ].join('\n');

  const isCEO = audience === 'ceo';
  const section = isCEO ? 'ceo:cockpit' : 'cfo:cockpit';
  const title = isCEO ? 'Board Summary' : 'Executive Narrative';
  const button = isCEO ? 'Generate board summary' : 'Generate executive narrative';
  const glyph = isCEO ? '📊' : '📝';
  const tone = isCEO ? 'rose' : 'purple';
  const systemPrompt = isCEO
    ? `You write a 1-paragraph board-ready summary of company financial health for a CEO presenting to a board of directors. Be high-level, formal, and forward-looking. Mention cash, expenses, and pipeline in plain numbers. End with one sentence on the top strategic risk. Output ONE paragraph, no bullet points, no markdown. Respond in English.`
    : `You write a 2-paragraph executive narrative summarizing the company financial position for a CFO cockpit. Paragraph 1 covers cash and liquidity. Paragraph 2 covers expenses, liabilities, and pipeline. Thai language, formal tone, no bullet points, no markdown. End each paragraph with a concrete number.`;

  return (
    <div className="p-4 bg-paper-2/50 border border-rule/70 rounded-md">
      <div className="text-xs font-mono font-bold uppercase text-ink-2 mb-3 tracking-wider flex items-center gap-2">
        <span>{glyph}</span> {isCEO ? 'CEO · Board Summary' : 'CFO · Executive Narrative'}
      </div>
      <AiActionButton
        sectionKey={section}
        task="chat"
        systemPrompt={systemPrompt}
        input={`Financial snapshot:\n${summary}`}
        buttonLabel={button}
        resultTitle={title}
        tone={tone as any}
        glyph={glyph}
        size="md"
      />
    </div>
  );
};