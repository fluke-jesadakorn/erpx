"use client";
import WorkspaceLink from '@erpx/workplace/components/workspace-link';
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Button } from "@erpx/ui/components/foundation";

import { useState } from "react";
import type {SocialPurchase} from "@erpx/server-collaboration/public/rules/social-presentation";
import {SocialCommandRecovery} from "@erpx/feature-collaboration/components/social-command-recovery";
import { useSocialCommand } from "@erpx/feature-collaboration/components/social-command";
import { SocialTextBody } from "@erpx/feature-collaboration/components/social-text-body";
import type { RichContent } from "@erpx/feature-shared/lib/home-rich-content";
import { AlertCircle, RotateCcw, Settings2, Sparkles } from "lucide-react";

type AgentReply = {
  id: string;
  state: string;
  error: string | null;
  actor_id: string;
  requester_name: string | null;
  attempt: number;
};
export function SocialAgentStatus({
  reply,
  api,
  userId,
  onChanged,
  answer,
  aiName,
  settingsUrl,
  purchase,
}: {
  reply: AgentReply;
  api: string;
  userId: string;
  onChanged: () => Promise<void>;
  answer?: { body: string; richContent: RichContent | null; author_name: string };
  aiName: string;
  settingsUrl?: string;
  purchase?:SocialPurchase;
}) {
  const command=useSocialCommand();const {run,send,busy,message}=command;
  const [retrying, setRetrying] = useState(false);
  if (reply.state === "completed")
    return answer ? (
      <div className="beta-agent-status">
        <div className="beta-agent-status-heading">
          <Sparkles size={16} aria-hidden="true" />
          <strong>
            {answer.author_name}
            {answer.author_name === "AI" ? "" : " · AI"}
          </strong>
          <small>มอบหมายโดย {reply.requester_name ?? "ผู้ร่วมสนทนา"}</small>
        </div>
        <SocialTextBody appearance="standard"
          body={answer.body}
          base={api.replace(/^\/api\/v1/, "")}
          doc={
            answer.richContent?.blocks[0]?.type === "text" ? answer.richContent.blocks[0].doc : null
          }
        />
      </div>
    ) : null;
  return (
    <InlineNotice tone="info" className="beta-agent-status" role="status" aria-live="polite">
      <div className="beta-agent-status-heading">
        <Sparkles size={16} aria-hidden="true" />
        <strong>
          {aiName}
          {aiName === "AI" ? "" : " · AI"}
        </strong>
        <small>มอบหมายโดย {reply.requester_name ?? "ผู้ร่วมสนทนา"}</small>
      </div>
      <p className="beta-agent-status-message">
        {reply.state === "pending" ? (
          "รอ AI ตอบ…"
        ) : reply.state === "running" ? (
          <span>
            AI กำลังตอบ… <span className="beta-ai-dot">●</span>
          </span>
        ) : (
          <>
            <AlertCircle size={16} aria-hidden="true" />
            {reply.error ?? "AI ตอบไม่ได้ในขณะนี้"}
          </>
        )}
      </p>
      {reply.state === "failed" && (
        <div className="beta-agent-status-actions">
          {reply.actor_id === userId && (
            <Button appearance="standard"
              variant="ghost"
              type="button"
              disabled={busy || retrying||purchase?.settleExisting===false||!!command.retryAttempt}
              onClick={() => {
                setRetrying(true);
                void run(async () => {
                  try {
                    await send(`${api}/beta/agent-replies/${reply.id}`, {});
                    await onChanged();
                  } finally {
                    setRetrying(false);
                  }
                });
              }}
            >
              <RotateCcw size={15} aria-hidden="true" />
              {busy || retrying ? "กำลังส่ง…" : "ลองอีกครั้ง"}
            </Button>
          )}
          {settingsUrl && (
            <WorkspaceLink href={settingsUrl}>
              <Settings2 size={15} aria-hidden="true" />
              ตั้งค่า AI
            </WorkspaceLink>
          )}
        </div>
      )}
      <SocialCommandRecovery command={command} history={purchase?.history!==false}/>
      {message && <small className="beta-agent-status-feedback">{message}</small>}
    </InlineNotice>
  );
}
