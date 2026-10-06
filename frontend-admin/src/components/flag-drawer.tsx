import { useEffect, useId, useMemo, useRef, useState } from "react";
import { toast } from "sonner";

import { Badge, Button, Input, Textarea } from "@/components/ui";
import { type FeatureFlagRow, type FlagState, flagStates } from "@/entity/schemas";
import { useUpdateFeatureFlag, useUsers } from "@/lib/queries";
import { userLabel } from "@/lib/utils";

function sameSet(a: string[], b: string[]): boolean {
  if (a.length !== b.length) return false;
  const setB = new Set(b);
  return a.every((x) => setB.has(x));
}

function stateLabel(state: FlagState): string {
  if (state === "loggedIn") return "Logged in";
  return state.charAt(0).toUpperCase() + state.slice(1);
}

export function FlagDrawer({ flag, onClose }: { flag: FeatureFlagRow; onClose: () => void }) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const users = useUsers();
  const update = useUpdateFeatureFlag();

  const [state, setState] = useState<FlagState>(flag.state);
  const [description, setDescription] = useState(flag.description);
  const [userIds, setUserIds] = useState<string[]>(flag.scopedUsers.map((u) => u.id));
  const [query, setQuery] = useState("");

  useEffect(() => {
    setState(flag.state);
    setDescription(flag.description);
    setUserIds(flag.scopedUsers.map((u) => u.id));
    setQuery("");
  }, [flag]);

  useEffect(() => {
    const dialog = dialogRef.current;
    const previousFocus = document.activeElement;
    dialog?.showModal();
    dialog?.querySelector<HTMLButtonElement>("aside button")?.focus();
    return () => {
      dialog?.close();
      if (previousFocus instanceof HTMLElement && previousFocus.isConnected) {
        previousFocus.focus();
      }
    };
  }, []);

  const originalUserIds = flag.scopedUsers.map((u) => u.id);
  const dirty =
    state !== flag.state || description !== flag.description || !sameSet(userIds, originalUserIds);

  const selectedUsers = useMemo(
    () =>
      userIds.map(
        (id) =>
          users.data?.find((u) => u.id === id) ??
          flag.scopedUsers.find((u) => u.id === id) ?? {
            id,
            displayName: null,
            email: null,
            providers: [],
            createdAt: "",
            noticeCount: 0,
          },
      ),
    [userIds, users.data, flag.scopedUsers],
  );

  const results = useMemo(() => {
    const q = query.trim().toLowerCase();
    return (users.data ?? [])
      .filter((u) => !userIds.includes(u.id))
      .filter(
        (u) =>
          q === "" || `${u.displayName ?? ""} ${u.email ?? ""} ${u.id}`.toLowerCase().includes(q),
      )
      .slice(0, 8);
  }, [users.data, userIds, query]);

  async function save() {
    try {
      await update.mutateAsync({ id: flag.id, body: { state, description, userIds } });
      toast.success("Flag saved");
    } catch (e) {
      toast.error(`Failed: ${e instanceof Error ? e.message : ""}`);
    }
  }

  return (
    <dialog
      ref={dialogRef}
      aria-labelledby={titleId}
      className="fixed inset-0 m-0 h-dvh max-h-none w-screen max-w-none bg-transparent p-0 text-inherit backdrop:bg-black/50"
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
      onKeyDown={(event) => {
        if (event.key !== "Tab") return;
        const controls = Array.from(
          event.currentTarget.querySelectorAll<HTMLElement>(
            "button, input, textarea, select, a[href], [tabindex]",
          ),
        ).filter(
          (element) =>
            element.tabIndex >= 0 &&
            !element.matches(":disabled") &&
            element.getClientRects().length > 0,
        );
        const first = controls[0];
        const last = controls[controls.length - 1];
        if (event.shiftKey && document.activeElement === first) {
          event.preventDefault();
          last?.focus();
        } else if (!event.shiftKey && document.activeElement === last) {
          event.preventDefault();
          first?.focus();
        }
      }}
    >
      <div className="flex h-full justify-end">
        <button
          type="button"
          aria-label="Close"
          tabIndex={-1}
          className="absolute inset-0 cursor-default"
          onClick={onClose}
        />
        <aside className="bg-bg border-border-soft relative flex h-full w-full max-w-md flex-col gap-4 overflow-y-auto border-l p-6 shadow-2xl">
          <div className="flex items-start justify-between gap-2">
            <div className="flex flex-wrap items-center gap-2">
              <h2 id={titleId} className="font-mono text-lg font-semibold text-[#f8fafc]">
                {flag.key}
              </h2>
              {flag.orphaned ? (
                <Badge className="bg-[#7f1d1d] text-[#fca5a5]">orphaned — not in code</Badge>
              ) : null}
            </div>
            <Button variant="secondary" aria-label="Close flag editor" onClick={onClose}>
              ✕
            </Button>
          </div>

          <div>
            <p className="text-text-muted mb-1 text-xs font-semibold tracking-wide uppercase">
              State
            </p>
            <div className="flex flex-wrap gap-1">
              {flagStates.map((s) => (
                <Button
                  key={s}
                  variant={state === s ? "primary" : "secondary"}
                  onClick={() => {
                    setState(s);
                  }}
                >
                  {stateLabel(s)}
                </Button>
              ))}
            </div>
          </div>

          <div>
            <p className="text-text-muted mb-1 text-xs font-semibold tracking-wide uppercase">
              Description
            </p>
            <Textarea
              value={description}
              onChange={(e) => {
                setDescription(e.target.value);
              }}
              placeholder="What does this flag gate?"
            />
          </div>

          <div>
            <p className="text-text-muted mb-1 text-xs font-semibold tracking-wide uppercase">
              Scoped users ({userIds.length})
            </p>
            <p className="text-text-dim mb-2 text-xs">Applies only while the state is Scoped.</p>
            <Input
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
              }}
              placeholder="Search users by name, email, id…"
            />
            {results.length > 0 ? (
              <div className="border-border-soft bg-surface mt-1 rounded border">
                {results.map((u) => (
                  <button
                    key={u.id}
                    type="button"
                    className="hover:bg-border block w-full cursor-pointer px-2 py-1.5 text-left text-xs"
                    onClick={() => {
                      setUserIds((prev) => [...prev, u.id]);
                      setQuery("");
                    }}
                  >
                    {userLabel(u)}
                  </button>
                ))}
              </div>
            ) : null}
            <div className="mt-2 flex flex-wrap gap-1">
              {selectedUsers.map((u) => (
                <span
                  key={u.id}
                  className="border-border bg-surface text-text inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-xs"
                >
                  {userLabel(u)}
                  <button
                    type="button"
                    aria-label={`Remove ${userLabel(u)}`}
                    className="text-text-dim cursor-pointer hover:text-[#fca5a5]"
                    onClick={() => {
                      setUserIds((prev) => prev.filter((id) => id !== u.id));
                    }}
                  >
                    ✕
                  </button>
                </span>
              ))}
              {selectedUsers.length === 0 ? (
                <span className="text-text-dim text-xs italic">No users selected</span>
              ) : null}
            </div>
          </div>

          <div className="mt-auto flex justify-end gap-2 pt-4">
            <Button variant="secondary" onClick={onClose}>
              Close
            </Button>
            <Button disabled={!dirty || update.isPending} onClick={() => void save()}>
              {update.isPending ? "Saving…" : "Save"}
            </Button>
          </div>
        </aside>
      </div>
    </dialog>
  );
}
