import { useState } from "react";
import { type OnChangeFn, type SortingState } from "@tanstack/react-table";

import { Button } from "@/components/ui";

export function usePageState(scope = "") {
  const initial = {
    scope,
    search: "",
    sort: "createdAt",
    descending: true,
    cursors: [undefined] as (string | undefined)[],
  };
  const [state, setState] = useState(initial);
  if (state.scope !== scope) setState(initial);
  const options = {
    cursor: state.cursors[state.cursors.length - 1],
    search: state.search,
    sort: state.sort,
    descending: state.descending,
  };
  const sorting: SortingState = [{ id: state.sort, desc: state.descending }];
  const setSorting: OnChangeFn<SortingState> = (updater) => {
    const next = typeof updater === "function" ? updater(sorting) : updater;
    const sort = next[0] ?? { id: "createdAt", desc: true };
    setState((current) => ({
      ...current,
      sort: sort.id,
      descending: sort.desc,
      cursors: [undefined],
    }));
  };
  return {
    options,
    sorting,
    setSorting,
    number: state.cursors.length,
    setSearch: (search: string) => {
      setState((current) => ({ ...current, search, cursors: [undefined] }));
    },
    reset: () => {
      setState((current) => ({ ...current, cursors: [undefined] }));
    },
    previous: () => {
      setState((current) => ({ ...current, cursors: current.cursors.slice(0, -1) }));
    },
    next: (cursor: string) => {
      setState((current) => ({ ...current, cursors: [...current.cursors, cursor] }));
    },
  };
}

export type PageState = ReturnType<typeof usePageState>;

export function PageControls({
  page,
  nextCursor,
  busy = false,
}: {
  page: PageState;
  nextCursor?: string | null;
  busy?: boolean;
}) {
  return (
    <div className="text-text-muted flex items-center gap-3 text-xs">
      <Button variant="secondary" disabled={busy || page.number === 1} onClick={page.previous}>
        Prev
      </Button>
      <span>Page {page.number}</span>
      <Button
        variant="secondary"
        disabled={busy || !nextCursor}
        onClick={() => {
          if (nextCursor) page.next(nextCursor);
        }}
      >
        Next
      </Button>
      <Button variant="secondary" disabled={busy || page.number === 1} onClick={page.reset}>
        First page
      </Button>
    </div>
  );
}
