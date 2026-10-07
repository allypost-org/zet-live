import { useNavigate, useSearch } from "@tanstack/react-router";
import { toast } from "sonner";

import { PageControls, usePageState } from "@/components/page-controls";
import { Button, Card, Input, CategoryBadge, Empty, Spinner, StatusBadge } from "@/components/ui";
import { type FeedbackFilter, feedbackFilterSchema } from "@/entity/schemas";
import {
  useArchiveFeedback,
  useClearAllFeedback,
  useDeleteFeedback,
  useDismissFeedback,
  useFeedback,
  useReplyFeedback,
} from "@/lib/queries";
import { confirmAction, promptText } from "@/lib/utils";

const FILTERS: FeedbackFilter[] = ["all", "new", "archived"];

export function FeedbackRoute() {
  const navigate = useNavigate();
  const search = useSearch({ strict: false });
  const filter = feedbackFilterSchema.catch("all").parse(search.handled ?? "all");

  const page = usePageState(filter);
  const { data, isLoading, isError, isFetching, refetch } = useFeedback(filter, page.options);
  const archive = useArchiveFeedback();
  const reply = useReplyFeedback();
  const dismiss = useDismissFeedback();
  const remove = useDeleteFeedback();
  const clearAll = useClearAllFeedback();

  function setFilter(next: FeedbackFilter) {
    void navigate({ to: "/feedback", search: { handled: next }, replace: true });
  }

  async function handleReply(id: number) {
    const text = promptText(`Reply to feedback #${id}:`);
    if (!text || !text.trim()) return;
    try {
      await reply.mutateAsync({ id, body: { reply: text.trim() } });
      toast.success("Reply sent");
    } catch (e) {
      toast.error(`Failed to send reply: ${e instanceof Error ? e.message : ""}`);
    }
  }

  async function handleDismiss(id: number) {
    if (!confirmAction(`Dismiss feedback #${id} (close without reply)?`)) return;
    try {
      await dismiss.mutateAsync({ id });
      toast.success("Feedback dismissed");
    } catch (e) {
      toast.error(`Failed to dismiss: ${e instanceof Error ? e.message : ""}`);
    }
  }

  async function handleDelete(id: number) {
    if (!confirmAction("Delete this feedback entry?")) return;
    try {
      await remove.mutateAsync({ id });
      toast.success("Feedback deleted");
    } catch (e) {
      toast.error(`Failed to delete: ${e instanceof Error ? e.message : ""}`);
    }
  }

  async function handleClearAll() {
    if (!confirmAction("Delete ALL feedback entries? This cannot be undone.")) return;
    try {
      await clearAll.mutateAsync();
      toast.success("All feedback cleared");
    } catch (e) {
      toast.error(`Failed: ${e instanceof Error ? e.message : ""}`);
    }
  }

  return (
    <div>
      <h1 className="mb-3 text-xl font-semibold text-[#f8fafc]">Feedback</h1>
      <Card>
        <div className="mb-2 flex items-center gap-1">
          {FILTERS.map((f) => (
            <Button
              key={f}
              variant={f === filter ? "primary" : "secondary"}
              onClick={() => {
                setFilter(f);
              }}
              className="px-2 py-1 text-[0.7rem]"
            >
              {f[0]?.toUpperCase()}
              {f.slice(1)}
            </Button>
          ))}
          <span className="flex-1" />
          <Button variant="secondary" onClick={() => void refetch()}>
            Refresh
          </Button>
          <Button variant="danger" onClick={() => void handleClearAll()}>
            Clear All
          </Button>
        </div>

        <Input
          placeholder="Search all feedback…"
          value={page.options.search}
          onChange={(e) => {
            page.setSearch(e.target.value);
          }}
          className="mb-3 max-w-xs"
        />
        <PageControls page={page} nextCursor={data?.nextCursor} busy={isFetching} />
        <div className="mt-3 flex flex-col gap-3">
          {isLoading ? (
            <Spinner />
          ) : isError ? (
            <Empty>Failed to load feedback</Empty>
          ) : !data || data.items.length === 0 ? (
            <Empty>No feedback yet</Empty>
          ) : (
            data.items.map((f) => {
              const archived = f.handled || f.dismissed;
              return (
                <div
                  key={f.id}
                  className={`border-border bg-bg rounded-lg border p-3 ${archived ? "opacity-55" : ""}`}
                >
                  <div className="mb-2 flex flex-wrap items-center gap-2">
                    <CategoryBadge category={f.category} />
                    <StatusBadge status={f.status} />
                    <span className="text-text-dim font-mono text-[0.7rem]">
                      {new Date(f.createdAt).toLocaleString()}
                    </span>
                    <span className="text-text-dim font-mono text-[0.7rem]">
                      #{f.id} · {f.ip}
                    </span>
                  </div>

                  <p className="text-text mb-2 text-sm break-words whitespace-pre-wrap">
                    {f.message}
                  </p>

                  {(f.userDisplayName || f.userEmail) && (
                    <div className="mb-2 font-mono text-[0.75rem] text-[#cbd5e1]">
                      {f.userDisplayName || f.userEmail}
                    </div>
                  )}

                  {(f.name || f.contact) && (
                    <div className="mb-2 text-xs text-[#cbd5e1]">
                      {f.name && (
                        <>
                          <strong>Name:</strong> {f.name}
                        </>
                      )}
                      {f.name && f.contact ? " · " : ""}
                      {f.contact && (
                        <>
                          <strong>Contact:</strong> {f.contact}
                        </>
                      )}
                    </div>
                  )}

                  {(f.metaUrl || f.metaLang || f.metaBuild || f.metaUa) && (
                    <div className="text-text-dim mb-2 flex flex-wrap gap-x-4 gap-y-1 text-[0.7rem]">
                      {f.metaUrl && (
                        <span>
                          <strong className="text-text-muted font-medium">URL:</strong> {f.metaUrl}
                        </span>
                      )}
                      {f.metaLang && (
                        <span>
                          <strong className="text-text-muted font-medium">Lang:</strong>{" "}
                          {f.metaLang}
                        </span>
                      )}
                      {f.metaBuild && (
                        <span>
                          <strong className="text-text-muted font-medium">Build:</strong>{" "}
                          {f.metaBuild}
                        </span>
                      )}
                      {f.metaUa && (
                        <span>
                          <strong className="text-text-muted font-medium">UA:</strong> {f.metaUa}
                        </span>
                      )}
                    </div>
                  )}

                  {f.reply && (
                    <div
                      className="text-text mb-2 border-l-[3px] border-[#22c55e] pl-2 text-sm break-words whitespace-pre-wrap"
                      style={{ marginTop: "0.25rem" }}
                    >
                      <strong>Reply:</strong> {f.reply}
                      {f.repliedAt && (
                        <span className="text-text-muted ml-1 text-[0.75rem]">
                          {new Date(f.repliedAt).toLocaleString()}
                        </span>
                      )}
                    </div>
                  )}

                  <div className="mt-2 flex gap-2">
                    <Button
                      variant="secondary"
                      className="px-2 py-1 text-[0.7rem]"
                      onClick={() => {
                        archive.mutate(
                          { id: f.id, body: { handled: !f.handled } },
                          {
                            onError: (e) => toast.error(`Failed: ${e.message}`),
                          },
                        );
                      }}
                    >
                      {f.handled ? "Unarchive" : "Archive"}
                    </Button>
                    <Button
                      className="px-2 py-1 text-[0.7rem]"
                      onClick={() => void handleReply(f.id)}
                    >
                      Reply
                    </Button>
                    <Button
                      variant="secondary"
                      className="px-2 py-1 text-[0.7rem]"
                      onClick={() => void handleDismiss(f.id)}
                    >
                      Dismiss
                    </Button>
                    <Button
                      variant="danger"
                      className="px-2 py-1 text-[0.7rem]"
                      onClick={() => void handleDelete(f.id)}
                    >
                      Delete
                    </Button>
                  </div>
                </div>
              );
            })
          )}
        </div>
      </Card>
    </div>
  );
}
