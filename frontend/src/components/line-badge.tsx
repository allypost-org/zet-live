type Props = {
  routeId: string;
  className?: string;
};

export function LineBadge({ routeId, className }: Props) {
  const isBus = routeId.length > 2;
  return (
    <span
      className={`text-on-primary inline-flex shrink-0 items-center rounded px-1.5 py-0.5 text-xs font-bold ${isBus ? "bg-primary" : "bg-danger"} ${className ?? ""}`}
    >
      {routeId}
    </span>
  );
}
