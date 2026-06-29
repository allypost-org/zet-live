import { useState, type ReactNode } from "react";
import { cn } from "@/utils/style";

/**
 * An avatar image that gracefully falls back to `fallback` when there is no
 * `src` or the image fails to load (e.g. Microsoft's userinfo returns an
 * auth-required Graph URL that an `<img>` can't load).
 */
export function Avatar({
  src,
  alt = "",
  className,
  fallback,
  fallbackClassName,
  fallbackInCorner,
}: {
  src: string | null | undefined;
  alt?: string;
  className?: string;
  fallback: ReactNode;
  fallbackClassName?: string;
  fallbackInCorner?: boolean;
}) {
  const [broken, setBroken] = useState(false);

  if (!src || broken) {
    return (
      <div
        className={cn(
          "bg-surface flex shrink-0 items-center justify-center overflow-hidden",
          fallbackClassName,
        )}
      >
        {fallback}
      </div>
    );
  }

  return (
    <div className="bg-surface relative rounded-full">
      <img
        src={src}
        alt={alt}
        referrerPolicy="no-referrer"
        className={cn("shrink-0", className)}
        onError={() => {
          setBroken(true);
        }}
      />
      {fallbackInCorner ? (
        <div className="bg-surface absolute right-0 bottom-0 h-1/2 w-1/2 translate-1/3 rounded-full p-px *:h-full *:w-full">
          {fallback}
        </div>
      ) : null}
    </div>
  );
}

/** Generic person glyph, used as the avatar fallback for the account header / button. */
export function UserGlyph({ size = 18 }: { size?: number }) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
      <circle cx="12" cy="7" r="4" />
    </svg>
  );
}
