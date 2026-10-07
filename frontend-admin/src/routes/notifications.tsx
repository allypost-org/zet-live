import { useState } from "react";
import { toast } from "sonner";

import { PageControls, usePageState } from "@/components/page-controls";
import { Button, Card, Input, Select } from "@/components/ui";
import {
  type NotificationTarget,
  type ToastType,
  notificationTargetSchema,
  toastTypeSchema,
} from "@/entity/schemas";
import { useConnections, useSendNotify, useUsers } from "@/lib/queries";
import { userLabel } from "@/lib/utils";

export function NotificationsRoute() {
  const { data: connections } = useConnections();
  const page = usePageState();
  const users = useUsers(page.options);
  const send = useSendNotify();

  const [message, setMessage] = useState("");
  const [type, setType] = useState<ToastType>("info");
  const [target, setTarget] = useState<NotificationTarget>("all");
  const [selectedIps, setSelectedIps] = useState<Set<string>>(new Set());
  const [accountId, setAccountId] = useState("");
  const [accountLabel, setAccountLabel] = useState("");

  const ipEntries = Object.entries(connections ?? []);
  const userOptions = users.data?.items ?? [];
  const effectiveAccountId = accountId;

  async function submit() {
    if (!message.trim()) return;
    if (target === "ips" && selectedIps.size === 0) {
      toast.error("Select at least one IP");
      return;
    }
    if (target === "account" && !effectiveAccountId) {
      toast.error("Select an account");
      return;
    }
    try {
      await send.mutateAsync({
        message: message.trim(),
        type,
        target,
        ips: target === "ips" ? [...selectedIps] : undefined,
        account: target === "account" ? effectiveAccountId : undefined,
      });
      toast.success("Notification sent");
      setMessage("");
      setSelectedIps(new Set());
    } catch (e) {
      toast.error(`Failed to send: ${e instanceof Error ? e.message : ""}`);
    }
  }

  return (
    <div>
      <h1 className="mb-3 text-xl font-semibold text-[#f8fafc]">Send Notification</h1>
      <Card>
        <div className="flex flex-col gap-3">
          <Input
            type="text"
            placeholder="Notification message…"
            value={message}
            onChange={(e) => {
              setMessage(e.target.value);
            }}
          />
          <div className="flex items-end gap-2">
            <Select
              value={type}
              onChange={(e) => {
                setType(toastTypeSchema.parse(e.target.value));
              }}
            >
              <option value="info">Info</option>
              <option value="success">Success</option>
              <option value="warning">Warning</option>
              <option value="error">Error</option>
            </Select>
            <Select
              value={target}
              onChange={(e) => {
                setTarget(notificationTargetSchema.parse(e.target.value));
              }}
            >
              <option value="all">All Users</option>
              <option value="ips">Specific IPs</option>
              <option value="account">Specific Account</option>
            </Select>
            <Button onClick={() => void submit()}>Send</Button>
          </div>

          {target === "ips" && (
            <div className="border-border bg-bg rounded border p-2">
              <p className="text-text-muted mb-1 text-xs">Select IPs from active connections:</p>
              {ipEntries.length === 0 ? (
                <p className="text-text-dim text-sm italic">No active connections</p>
              ) : (
                <div className="flex flex-col">
                  {ipEntries.map(([ip]) => (
                    <label
                      key={ip}
                      className="text-text-muted flex cursor-pointer items-center gap-1 py-0.5 text-xs"
                    >
                      <input
                        type="checkbox"
                        checked={selectedIps.has(ip)}
                        onChange={(e) => {
                          setSelectedIps((prev) => {
                            const next = new Set(prev);
                            if (e.target.checked) next.add(ip);
                            else next.delete(ip);
                            return next;
                          });
                        }}
                      />
                      <span className="font-mono">{ip}</span>
                    </label>
                  ))}
                </div>
              )}
            </div>
          )}

          {target === "account" && (
            <div className="flex flex-col gap-2">
              <Input
                placeholder="Search all accounts…"
                value={page.options.search}
                onChange={(e) => {
                  page.setSearch(e.target.value);
                }}
              />
              <Select
                value={effectiveAccountId}
                onChange={(e) => {
                  setAccountId(e.target.value);
                  const selected = userOptions.find((u) => u.id === e.target.value);
                  setAccountLabel(selected ? userLabel(selected) : e.target.value);
                }}
              >
                <option value="">Select an account</option>
                {accountId && !userOptions.some((u) => u.id === accountId) && (
                  <option value={accountId}>{accountLabel}</option>
                )}
                {userOptions.map((u) => (
                  <option key={u.id} value={u.id}>
                    {userLabel(u)}
                  </option>
                ))}
              </Select>
              <PageControls
                page={page}
                nextCursor={users.data?.nextCursor}
                busy={users.isFetching}
              />
              {users.isError && <p className="text-text-dim text-xs">Failed to load accounts.</p>}
            </div>
          )}
        </div>
      </Card>
    </div>
  );
}
