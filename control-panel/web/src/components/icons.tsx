// Small, dependency-free inline SVG icons. All use `currentColor` so they
// inherit text color, and a common stroke style for a cohesive line-icon look.
import type { SVGProps } from 'react'

type IconProps = SVGProps<SVGSVGElement> & { size?: number }

function Base({ size = 18, children, ...rest }: IconProps & { children: React.ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...rest}
    >
      {children}
    </svg>
  )
}

export function CatalogIcon(props: IconProps) {
  return (
    <Base {...props}>
      <rect x="3" y="3" width="7" height="7" rx="1.5" />
      <rect x="14" y="3" width="7" height="7" rx="1.5" />
      <rect x="3" y="14" width="7" height="7" rx="1.5" />
      <rect x="14" y="14" width="7" height="7" rx="1.5" />
    </Base>
  )
}

export function SourcesIcon(props: IconProps) {
  return (
    <Base {...props}>
      <ellipse cx="12" cy="5" rx="8" ry="3" />
      <path d="M4 5v6c0 1.66 3.58 3 8 3s8-1.34 8-3V5" />
      <path d="M4 11v6c0 1.66 3.58 3 8 3s8-1.34 8-3v-6" />
    </Base>
  )
}

export function SearchIcon(props: IconProps) {
  return (
    <Base {...props}>
      <circle cx="11" cy="11" r="7" />
      <path d="m21 21-4.3-4.3" />
    </Base>
  )
}

export function CloseIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M18 6 6 18" />
      <path d="m6 6 12 12" />
    </Base>
  )
}

export function LockIcon(props: IconProps) {
  return (
    <Base {...props}>
      <rect x="4" y="10" width="16" height="10" rx="2" />
      <path d="M8 10V7a4 4 0 0 1 8 0v3" />
    </Base>
  )
}

export function RefreshIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M3 12a9 9 0 0 1 15-6.7L21 8" />
      <path d="M21 3v5h-5" />
      <path d="M21 12a9 9 0 0 1-15 6.7L3 16" />
      <path d="M3 21v-5h5" />
    </Base>
  )
}

export function EyeIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7-10-7-10-7Z" />
      <circle cx="12" cy="12" r="3" />
    </Base>
  )
}

export function TableIcon(props: IconProps) {
  return (
    <Base {...props}>
      <rect x="3" y="4" width="18" height="16" rx="2" />
      <path d="M3 10h18" />
      <path d="M3 15h18" />
      <path d="M9 10v10" />
    </Base>
  )
}

export function LayersIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="m12 3 9 5-9 5-9-5 9-5Z" />
      <path d="m3 13 9 5 9-5" />
    </Base>
  )
}

export function BoltIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M13 2 4 14h7l-1 8 9-12h-7l1-8Z" />
    </Base>
  )
}

export function RowsIcon(props: IconProps) {
  return (
    <Base {...props}>
      <rect x="3" y="4" width="18" height="4" rx="1" />
      <rect x="3" y="10" width="18" height="4" rx="1" />
      <rect x="3" y="16" width="18" height="4" rx="1" />
    </Base>
  )
}

export function ChartIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M3 3v18h18" />
      <rect x="7" y="12" width="3" height="6" rx="0.5" />
      <rect x="12" y="8" width="3" height="10" rx="0.5" />
      <rect x="17" y="5" width="3" height="13" rx="0.5" />
    </Base>
  )
}

export function ExternalIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M15 3h6v6" />
      <path d="M10 14 21 3" />
      <path d="M21 14v5a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5" />
    </Base>
  )
}

// The BudBuk "BB" mark: a rounded square with the brand gradient.
export function Logo({ size = 30 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 32 32" aria-hidden="true">
      <defs>
        <linearGradient id="bb-grad" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#2A63A6" />
          <stop offset="1" stopColor="#1E4E86" />
        </linearGradient>
      </defs>
      <rect x="0" y="0" width="32" height="32" rx="8" fill="url(#bb-grad)" />
      <text
        x="16"
        y="17"
        textAnchor="middle"
        dominantBaseline="central"
        fontFamily="-apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif"
        fontSize="13"
        fontWeight="700"
        letterSpacing="0.5"
        fill="#F6883B"
      >
        BB
      </text>
    </svg>
  )
}
