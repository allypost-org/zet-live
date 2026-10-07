import type {
  ComponentProps,
  InputHTMLAttributes,
  ReactNode,
  SelectHTMLAttributes,
  TextareaHTMLAttributes,
} from "react";

import { cn } from "@/lib/utils";

export function Card({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div className={cn("border-border-soft bg-surface rounded-lg border p-4 shadow-sm", className)}>
      {children}
    </div>
  );
}

export function SectionTitle({ children }: { children: ReactNode }) {
  return (
    <h2 className="border-border-soft text-text-muted mt-8 mb-3 border-b pb-2 text-sm font-semibold tracking-wide uppercase first:mt-0">
      {children}
    </h2>
  );
}

type ButtonVariant = "primary" | "secondary" | "danger";

const buttonVariants: Record<ButtonVariant, string> = {
  primary: "bg-primary text-white hover:bg-primary-hover",
  secondary: "bg-border text-text hover:bg-[#475569]",
  danger: "bg-[#dc2626] text-white hover:bg-[#b91c1c]",
};

export function Button({
  variant = "primary",
  className,
  ...props
}: ComponentProps<"button"> & { variant?: ButtonVariant }) {
  return (
    <button
      className={cn(
        "inline-flex cursor-pointer items-center justify-center gap-1.5 rounded px-3 py-1.5 text-xs font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50",
        buttonVariants[variant],
        className,
      )}
      {...props}
    />
  );
}

export function Toggle({
  checked,
  onChange,
  title,
}: {
  checked: boolean;
  onChange: (next: boolean) => void;
  title?: string;
}) {
  return (
    <button
      type="button"
      title={title}
      role="switch"
      aria-checked={checked}
      onClick={() => {
        onChange(!checked);
      }}
      className={cn(
        "relative h-[22px] w-10 shrink-0 cursor-pointer rounded-full transition-colors",
        checked ? "bg-primary" : "bg-border",
      )}
    >
      <span
        className={cn(
          "absolute top-0.5 left-0.5 h-[18px] w-[18px] rounded-full bg-white transition-transform",
          checked && "translate-x-[18px]",
        )}
      />
    </button>
  );
}

export function Input({ className, ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      className={cn(
        "border-border bg-bg font-inherit text-text focus:border-primary min-w-0 flex-1 rounded border px-2 py-1.5 text-xs outline-none",
        className,
      )}
      {...props}
    />
  );
}

export function Textarea({ className, ...props }: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return (
    <textarea
      className={cn(
        "border-border bg-bg font-inherit text-text focus:border-primary min-h-[60px] w-full resize-y rounded border px-2 py-1.5 text-xs outline-none",
        className,
      )}
      {...props}
    />
  );
}

export function Select({ className, ...props }: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <select
      className={cn(
        "border-border bg-bg font-inherit text-text focus:border-primary rounded border px-2 py-1.5 text-xs outline-none",
        className,
      )}
      {...props}
    />
  );
}

export function Badge({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <span
      className={cn(
        "inline-block shrink-0 rounded-full px-2 py-0.5 text-[0.7rem] font-semibold uppercase",
        className,
      )}
    >
      {children}
    </span>
  );
}

const severityBadgeClass: Record<string, string> = {
  info: "bg-[#1e3a5f] text-[#93c5fd]",
  warning: "bg-[#713f12] text-[#fde68a]",
  error: "bg-[#7f1d1d] text-[#fca5a5]",
};

export function SeverityBadge({ severity }: { severity: string }) {
  return (
    <Badge className={severityBadgeClass[severity] ?? severityBadgeClass.info!}>{severity}</Badge>
  );
}

const statusClass: Record<string, string> = {
  success: "bg-[#065f46] text-[#6ee7b7]",
  error: "bg-[#7f1d1d] text-[#fca5a5]",
  "in-progress": "bg-[#1e3a5f] text-[#93c5fd]",
  paused: "bg-[#374151] text-[#d1d5db]",
  skipped: "bg-[#374151] text-[#d1d5db]",
  acknowledged: "bg-[#374151] text-[#d1d5db]",
  replied: "bg-[#065f46] text-[#6ee7b7]",
  dismissed: "bg-[#7f1d1d] text-[#fca5a5]",
  open: "bg-[#1e3a5f] text-[#93c5fd]",
};

export function StatusBadge({ status }: { status: string }) {
  return (
    <Badge className={statusClass[status] ?? statusClass.open!}>{status.replace("-", " ")}</Badge>
  );
}

const feedbackCategoryClass: Record<string, string> = {
  bug: "bg-[#7f1d1d] text-[#fca5a5]",
  feature: "bg-[#065f46] text-[#6ee7b7]",
  other: "bg-[#374151] text-[#d1d5db]",
};

export function CategoryBadge({ category }: { category: string }) {
  return (
    <Badge className={feedbackCategoryClass[category] ?? feedbackCategoryClass.other!}>
      {category}
    </Badge>
  );
}

export function Empty({ children = "Nothing here yet." }: { children?: ReactNode }) {
  return <p className="text-text-dim text-sm italic">{children}</p>;
}

export function Spinner({ label = "Loading…" }: { label?: string }) {
  return (
    <div className="text-text-muted flex items-center gap-2 text-sm">
      <span className="border-border border-t-primary inline-block h-4 w-4 animate-spin rounded-full border-2" />
      {label}
    </div>
  );
}

export function Row({ children }: { children: ReactNode }) {
  return (
    <div className="border-border flex items-center gap-3 border-b py-2 last:border-b-0">
      {children}
    </div>
  );
}

export function Label({ children }: { children: ReactNode }) {
  return <span className="w-[200px] shrink-0 text-sm font-medium text-[#cbd5e1]">{children}</span>;
}

export function ErrorText({ children }: { children: ReactNode }) {
  return <p className="text-sm text-[#fca5a5]">{children}</p>;
}
