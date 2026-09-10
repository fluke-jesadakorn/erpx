"use client";
import { EmptyState } from "@/components/ui/primitives";
export default function WorkspaceError({ reset }: { reset: () => void }) {
  return (
    <main className="content">
      <div className="card">
        <EmptyState
          title="We couldn’t load this view"
          description="Your work is still there. Try again to reconnect to your workspace."
          action={
            <button className="button" onClick={reset}>
              Try again
            </button>
          }
        />
      </div>
    </main>
  );
}
