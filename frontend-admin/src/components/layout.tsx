import { type ReactNode } from "react";
import { Link, Outlet, useRouterState } from "@tanstack/react-router";

import { clearCredentials } from "@/lib/auth";
import { cn } from "@/lib/utils";

interface NavItem {
  to: string;
  label: string;
}

const NAV: NavItem[] = [
  { to: "/", label: "Dashboard" },
  { to: "/settings", label: "Settings" },
  { to: "/auth", label: "Auth" },
  { to: "/feature-flags", label: "Feature Flags" },
  { to: "/notices", label: "Notices" },
  { to: "/notifications", label: "Send Notification" },
  { to: "/sync", label: "Sync" },
  { to: "/feedback", label: "Feedback" },
];

export function AuthLayout({ children }: { children?: ReactNode }) {
  const pathname = useRouterState({ select: (s) => s.location.pathname });

  return (
    <div className="mx-auto flex min-h-full max-w-6xl gap-6 p-6">
      <aside className="w-52 shrink-0">
        <h1 className="mb-4 text-lg font-semibold text-[#f8fafc]">ZET Live Admin</h1>
        <nav className="flex flex-col gap-0.5">
          {NAV.map((item) => {
            const active = item.to === "/" ? pathname === "/" : pathname.startsWith(item.to);
            return (
              <Link
                key={item.to}
                to={item.to}
                className={cn(
                  "rounded-md px-3 py-1.5 text-sm transition-colors",
                  active
                    ? "bg-primary text-white"
                    : "text-text-muted hover:bg-surface hover:text-text",
                )}
              >
                {item.label}
              </Link>
            );
          })}
        </nav>
        <button
          type="button"
          onClick={() => {
            clearCredentials();
            window.location.href = "/login";
          }}
          className="text-text-dim mt-6 cursor-pointer text-left text-xs hover:text-[#fca5a5]"
        >
          Sign out
        </button>
      </aside>
      <main className="min-w-0 flex-1">{children ?? <Outlet />}</main>
    </div>
  );
}
