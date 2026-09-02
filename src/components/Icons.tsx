/** Lightweight inline SVG icons (16×16). */

import type { ReactNode } from "react";

type IconProps = {
  size?: number;
  className?: string;
  title?: string;
};

function Svg({
  size = 16,
  className,
  title,
  children,
}: IconProps & { children: ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden={title ? undefined : true}
      role={title ? "img" : undefined}
    >
      {title ? <title>{title}</title> : null}
      {children}
    </svg>
  );
}

export function IconPlus(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M8 3v10M3 8h10" />
    </Svg>
  );
}

export function IconHistory(p: IconProps) {
  return (
    <Svg {...p}>
      <circle cx="8" cy="8" r="5.5" />
      <path d="M8 5v3.5l2 1.5" />
    </Svg>
  );
}

export function IconBroom(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M4 13h8M5.5 13l1-7h3l1 7M7 6V3.5a1 1 0 0 1 2 0V6" />
    </Svg>
  );
}

export function IconSettings(p: IconProps) {
  return (
    <Svg {...p}>
      <circle cx="8" cy="8" r="2" />
      <path d="M8 2.5v1.5M8 12v1.5M2.5 8H4M12 8h1.5M4.2 4.2l1.1 1.1M10.7 10.7l1.1 1.1M4.2 11.8l1.1-1.1M10.7 5.3l1.1-1.1" />
    </Svg>
  );
}

export function IconGrid(p: IconProps) {
  return (
    <Svg {...p}>
      <rect x="2.5" y="2.5" width="4.5" height="4.5" rx="0.8" />
      <rect x="9" y="2.5" width="4.5" height="4.5" rx="0.8" />
      <rect x="2.5" y="9" width="4.5" height="4.5" rx="0.8" />
      <rect x="9" y="9" width="4.5" height="4.5" rx="0.8" />
    </Svg>
  );
}

export function IconMore(p: IconProps) {
  return (
    <Svg {...p}>
      <circle cx="3.5" cy="8" r="1" fill="currentColor" stroke="none" />
      <circle cx="8" cy="8" r="1" fill="currentColor" stroke="none" />
      <circle cx="12.5" cy="8" r="1" fill="currentColor" stroke="none" />
    </Svg>
  );
}

export function IconTerminal(p: IconProps) {
  return (
    <Svg {...p}>
      <rect x="2" y="3" width="12" height="10" rx="1.5" />
      <path d="M4.5 6.5 6.5 8l-2 1.5M8 10.5h3.5" />
    </Svg>
  );
}

export function IconCode(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M5.5 4.5 2.5 8l3 3.5M10.5 4.5l3 3.5-3 3.5M9 3.5l-2 9" />
    </Svg>
  );
}

export function IconReview(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M3 3.5h7.5L13 6.5v6A1.5 1.5 0 0 1 11.5 14h-7A1.5 1.5 0 0 1 3 12.5v-9A1.5 1.5 0 0 1 4.5 2" />
      <path d="M10 3.5V6h3M5.5 9h5M5.5 11.5h3.5" />
    </Svg>
  );
}

export function IconChevronDown(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M4 6.5 8 10.5 12 6.5" />
    </Svg>
  );
}

export function IconTool(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M10.5 3.5a2.5 2.5 0 0 0-3.4 3.4L3 11v2h2l4.1-4.1a2.5 2.5 0 0 0 3.4-3.4L11 7l-1.5-1.5 1-2Z" />
    </Svg>
  );
}

export function IconStar(p: IconProps) {
  return (
    <Svg {...p}>
      <path
        d="M8 1.4 9.6 6.2 14.6 8 9.6 9.8 8 14.6 6.4 9.8 1.4 8 6.4 6.2Z"
        fill="currentColor"
        stroke="none"
      />
    </Svg>
  );
}

export function IconAgent(p: IconProps) {
  return (
    <Svg {...p}>
      <circle cx="8" cy="6" r="2.5" />
      <path d="M3.5 13c.8-2.2 2.4-3.5 4.5-3.5s3.7 1.3 4.5 3.5" />
    </Svg>
  );
}
