import { type ColumnDef } from "@tanstack/react-table";
import { useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import { toast } from "sonner";

import { usePageState } from "@/components/page-controls";
import { DataTable } from "@/components/data-table";
import { ProviderForm } from "@/components/provider-form";
import { Badge, Button, Card, Empty, Row, SectionTitle, Spinner, Toggle } from "@/components/ui";
import { type AuthProvider, type AdminSessionInfo, type UserSummary } from "@/entity/schemas";
import {
  useAuthProviders,
  useCreateAuthProvider,
  useDeleteAuthProvider,
  useDeleteSession,
  useSessions,
  useUpdateAuthProvider,
  useUsers,
} from "@/lib/queries";
import { confirmAction, promptText } from "@/lib/utils";

function ProvidersSection() {
  const { data, isLoading, isError } = useAuthProviders();
  const create = useCreateAuthProvider();
  const update = useUpdateAuthProvider();
  const remove = useDeleteAuthProvider();

  const providers = data?.providers ?? [];
  const presets = data?.presets ?? [];
  const available = presets.filter((p) => !providers.some((x) => x.id === p.id));
  const redirectUri = `${window.location.origin}/api/v1/auth/<provider>/callback`;

  async function handleCreate(values: { id: string; clientId: string; clientSecret: string }) {
    try {
      await create.mutateAsync(values);
      toast.success("Provider added");
    } catch (e) {
      toast.error(`Failed to add provider: ${e instanceof Error ? e.message : ""}`);
    }
  }

  async function handleToggle(id: string, enabled: boolean) {
    try {
      await update.mutateAsync({ id, body: { enabled: !enabled } });
      toast.success(enabled ? "Provider disabled" : "Provider enabled");
    } catch (e) {
      toast.error(`Failed: ${e instanceof Error ? e.message : ""}`);
    }
  }

  async function handleEdit(id: string) {
    const newClientId = promptText(`New Client ID for ${id} (cancel to keep current):`);
    const newSecret = promptText(`New Client Secret for ${id} (cancel to keep current):`);
    const body: { clientId?: string; clientSecret?: string } = {};
    if (newClientId !== null) body.clientId = newClientId.trim();
    if (newSecret !== null) body.clientSecret = newSecret;
    if (Object.keys(body).length === 0) return;
    try {
      await update.mutateAsync({ id, body });
      toast.success("Provider updated");
    } catch (e) {
      toast.error(`Failed: ${e instanceof Error ? e.message : ""}`);
    }
  }

  async function handleDelete(id: string) {
    if (!confirmAction(`Remove the ${id} provider configuration?`)) return;
    try {
      await remove.mutateAsync(id);
      toast.success("Provider removed");
    } catch (e) {
      toast.error(`Failed: ${e instanceof Error ? e.message : ""}`);
    }
  }

  return (
    <Card>
      <SectionTitle>Auth Providers</SectionTitle>
      {isLoading ? (
        <Spinner />
      ) : isError ? (
        <Empty>Failed to load auth providers.</Empty>
      ) : providers.length === 0 ? (
        <Empty>No auth providers configured — user accounts are disabled.</Empty>
      ) : (
        providers.map((p: AuthProvider) => (
          <Row key={p.id}>
            <span className="w-[120px] shrink-0 text-sm font-medium text-[#cbd5e1]">{p.name}</span>
            <span className="text-text-muted flex-1 truncate overflow-hidden font-mono text-[0.7rem]">
              {p.clientId}
            </span>
            <Toggle
              checked={p.enabled}
              onChange={() => void handleToggle(p.id, p.enabled)}
              title={p.enabled ? "Enabled" : "Disabled"}
            />
            <Button variant="secondary" onClick={() => void handleEdit(p.id)}>
              Edit
            </Button>
            <Button variant="danger" onClick={() => void handleDelete(p.id)}>
              Delete
            </Button>
          </Row>
        ))
      )}

      <div className="border-border mt-3 flex flex-col gap-3 border-t pt-3">
        <ProviderForm availablePresets={available} onSubmit={handleCreate} />
        <p className="text-text-dim text-[0.7rem]">
          Redirect URI to register at the provider: <code className="font-mono">{redirectUri}</code>{" "}
          (uses APP_URL)
        </p>
      </div>
    </Card>
  );
}

function UsersTab() {
  const navigate = useNavigate();
  const page = usePageState();
  const { data, isLoading, isError, isFetching } = useUsers(page.options);

  const columns: ColumnDef<UserSummary>[] = [
    {
      header: "Name",
      id: "displayName",
      accessorFn: (u) => u.displayName ?? "",
      cell: ({ row }) => (
        <span className="font-medium text-[#cbd5e1]">{row.original.displayName || "—"}</span>
      ),
    },
    {
      header: "Email",
      id: "email",
      accessorFn: (u) => u.email ?? "",
      cell: ({ row }) => <span className="text-text-muted">{row.original.email || "—"}</span>,
    },
    {
      header: "Providers",
      id: "providers",
      accessorFn: (u) => u.providers.length,
      enableGlobalFilter: false,
      cell: ({ row }) =>
        row.original.providers.length === 0 ? (
          <span className="text-text-dim">—</span>
        ) : (
          <span className="text-text-muted">{row.original.providers.join(", ")}</span>
        ),
    },
    {
      header: "Joined",
      accessorKey: "createdAt",
      enableGlobalFilter: false,
      cell: ({ row }) => (
        <span className="text-text-dim text-xs">
          {new Date(row.original.createdAt).toLocaleDateString()}
        </span>
      ),
    },
    {
      header: "Notices",
      accessorKey: "noticeCount",
      enableGlobalFilter: false,
      cell: ({ row }) =>
        row.original.noticeCount > 0 ? (
          <Badge className="bg-[#713f12] text-[#fde68a]">{row.original.noticeCount}</Badge>
        ) : (
          <span className="text-text-dim">—</span>
        ),
    },
  ];

  return (
    <DataTable
      columns={columns}
      data={data?.items ?? []}
      serverPage={{ page, nextCursor: data?.nextCursor, busy: isFetching }}
      searchAccessor={(u) => `${u.displayName ?? ""} ${u.email ?? ""} ${u.providers.join(" ")}`}
      searchPlaceholder="Search by name, email, provider…"
      onRowClick={(u) => {
        void navigate({ to: "/auth/users/$id", params: { id: u.id } });
      }}
      emptyMessage={isError ? "Failed to load users." : isLoading ? "Loading…" : "No users."}
    />
  );
}

function SessionsTab() {
  const navigate = useNavigate();
  const page = usePageState();
  const { data, isLoading, isError, isFetching } = useSessions(page.options);
  const revoke = useDeleteSession();

  async function handleRevoke(id: string) {
    if (!confirmAction("Revoke this session? The user will be logged out.")) return;
    try {
      await revoke.mutateAsync(id);
      toast.success("Session revoked");
    } catch (e) {
      toast.error(`Failed to revoke session: ${e instanceof Error ? e.message : ""}`);
    }
  }

  const columns: ColumnDef<AdminSessionInfo>[] = [
    {
      header: "User",
      id: "user",
      accessorFn: (s) => s.userDisplayName || s.userEmail || s.userId.slice(0, 8),
      cell: ({ row }) => {
        const label =
          row.original.userDisplayName || row.original.userEmail || row.original.userId.slice(0, 8);
        return (
          <button
            type="button"
            className="text-primary text-left font-medium hover:underline"
            onClick={(e) => {
              e.stopPropagation();
              void navigate({ to: "/auth/users/$id", params: { id: row.original.userId } });
            }}
          >
            {label}
          </button>
        );
      },
    },
    {
      header: "IP",
      id: "ip",
      accessorFn: (s) => s.ip ?? "",
      cell: ({ row }) => (
        <span className="text-text-muted font-mono text-xs">{row.original.ip || "—"}</span>
      ),
    },
    {
      header: "User agent",
      id: "userAgent",
      accessorFn: (s) => s.userAgent ?? "",
      enableGlobalFilter: false,
      cell: ({ row }) => (
        <span className="text-text-muted block max-w-[280px] truncate text-xs">
          {row.original.userAgent || "—"}
        </span>
      ),
    },
    {
      header: "Created",
      accessorKey: "createdAt",
      enableGlobalFilter: false,
      cell: ({ row }) => (
        <span className="text-text-dim text-xs">
          {new Date(row.original.createdAt).toLocaleString()}
        </span>
      ),
    },
    {
      header: "Expires",
      accessorKey: "expiresAt",
      enableGlobalFilter: false,
      cell: ({ row }) => (
        <span className="text-text-dim text-xs">
          {new Date(row.original.expiresAt).toLocaleString()}
        </span>
      ),
    },
    {
      id: "actions",
      header: "",
      enableSorting: false,
      enableGlobalFilter: false,
      cell: ({ row }) => (
        <Button
          variant="danger"
          className="px-2 py-1 text-[0.7rem]"
          onClick={(e) => {
            e.stopPropagation();
            void handleRevoke(row.original.id);
          }}
        >
          Revoke
        </Button>
      ),
    },
  ];

  return (
    <DataTable
      columns={columns}
      data={data?.items ?? []}
      serverPage={{ page, nextCursor: data?.nextCursor, busy: isFetching }}
      searchPlaceholder="Search sessions…"
      emptyMessage={isError ? "Failed to load sessions." : isLoading ? "Loading…" : "No sessions."}
    />
  );
}

export function AuthRoute() {
  const [tab, setTab] = useState<"users" | "sessions">("users");

  return (
    <div>
      <h1 className="mb-3 text-xl font-semibold text-[#f8fafc]">Auth</h1>
      <ProvidersSection />

      <div className="mt-6 flex items-center gap-2">
        {(["users", "sessions"] as const).map((t) => (
          <Button
            key={t}
            variant={tab === t ? "primary" : "secondary"}
            onClick={() => {
              setTab(t);
            }}
          >
            {t === "users" ? "Users" : "Sessions"}
          </Button>
        ))}
      </div>
      <div className="mt-3">{tab === "users" ? <UsersTab /> : <SessionsTab />}</div>
    </div>
  );
}
