import type { ColumnDef } from "@tanstack/react-table";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { toast } from "sonner";

import { DataTable } from "@/components/data-table";
import { FlagDrawer } from "@/components/flag-drawer";
import { Badge, Button, Card, Empty, Spinner } from "@/components/ui";
import { type FeatureFlagRow, type FlagState } from "@/entity/schemas";
import { useDeleteFeatureFlag, useFeatureFlags } from "@/lib/queries";
import { confirmAction } from "@/lib/utils";

const FLAG_STATE_CLASS: Record<FlagState, string> = {
  disabled: "bg-[#374151] text-[#d1d5db]",
  enabled: "bg-[#065f46] text-[#6ee7b7]",
  loggedIn: "bg-[#1e3a5f] text-[#93c5fd]",
  scoped: "bg-[#713f12] text-[#fde68a]",
};

function FlagStateBadge({ state }: { state: FlagState }) {
  return (
    <Badge className={FLAG_STATE_CLASS[state]}>{state === "loggedIn" ? "logged in" : state}</Badge>
  );
}

export function FeatureFlagsRoute() {
  const navigate = useNavigate();
  const search = useSearch({ strict: false });
  const { data, isLoading, isError } = useFeatureFlags();
  const remove = useDeleteFeatureFlag();

  const flags = data ?? [];
  const flagKey = typeof search.flag === "string" ? search.flag : undefined;
  const selected = flags.find((f) => f.key === flagKey);

  async function handleDelete(flag: FeatureFlagRow) {
    if (!confirmAction(`Delete the "${flag.key}" flag? Its scoped users will be removed too.`)) {
      return;
    }
    try {
      await remove.mutateAsync(flag.id);
      void navigate({ to: "/feature-flags", search: {}, replace: true });
      toast.success("Flag deleted");
    } catch (e) {
      toast.error(`Failed: ${e instanceof Error ? e.message : ""}`);
    }
  }

  const columns: ColumnDef<FeatureFlagRow>[] = [
    {
      header: "Key",
      accessorKey: "key",
      cell: ({ row }) => <span className="font-mono text-[#cbd5e1]">{row.original.key}</span>,
    },
    {
      header: "Description",
      accessorKey: "description",
      cell: ({ row }) => <span className="text-text-muted">{row.original.description || "—"}</span>,
    },
    {
      header: "State",
      accessorKey: "state",
      cell: ({ row }) => <FlagStateBadge state={row.original.state} />,
    },
    {
      header: "Users",
      accessorFn: (f) => (f.state === "scoped" ? f.scopedUsers.length : -1),
      cell: ({ row }) => (row.original.state === "scoped" ? row.original.scopedUsers.length : "—"),
    },
    {
      header: "Updated",
      accessorFn: (f) => f.updatedAt,
      cell: ({ row }) => (
        <span className="text-text-dim">{new Date(row.original.updatedAt).toLocaleString()}</span>
      ),
    },
    {
      id: "actions",
      header: "",
      enableSorting: false,
      enableGlobalFilter: false,
      cell: ({ row }) =>
        row.original.orphaned ? (
          <div className="flex items-center gap-2">
            <Badge className="bg-[#7f1d1d] text-[#fca5a5]">orphaned</Badge>
            <Button
              variant="danger"
              className="px-2 py-1 text-[0.7rem]"
              onClick={(e) => {
                e.stopPropagation();
                void handleDelete(row.original);
              }}
            >
              Delete
            </Button>
          </div>
        ) : null,
    },
  ];

  return (
    <div>
      <h1 className="mb-3 text-xl font-semibold text-[#f8fafc]">Feature Flags</h1>
      <Card>
        {isLoading ? (
          <Spinner />
        ) : isError ? (
          <Empty>Failed to load feature flags.</Empty>
        ) : flags.length === 0 ? (
          <Empty>No feature flags.</Empty>
        ) : (
          <DataTable
            columns={columns}
            data={flags}
            searchAccessor={(f) => `${f.key} ${f.description}`}
            searchPlaceholder="Search flags…"
            onRowClick={(f) => {
              void navigate({ to: "/feature-flags", search: { flag: f.key } });
            }}
            emptyMessage="No feature flags."
          />
        )}
      </Card>
      {selected ? (
        <FlagDrawer
          flag={selected}
          onClose={() => {
            void navigate({ to: "/feature-flags", search: {}, replace: true });
          }}
        />
      ) : null}
    </div>
  );
}
