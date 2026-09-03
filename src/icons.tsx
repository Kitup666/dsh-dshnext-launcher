/** 侧边栏与空状态图标：统一 1.6px 线宽的描边图形，避免 emoji 在同一列里彩色/单色混排 */
export function Icon({ name, size = 18 }: { name: string; size?: number }) {
  const common = {
    width: size,
    height: size,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.7,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
  };
  switch (name) {
    case "launch":
      return (
        <svg {...common}>
          <path d="M12 3c3.5 2.2 5.4 5.7 5.4 9.6L12 18l-5.4-5.4C6.6 8.7 8.5 5.2 12 3Z" />
          <circle cx="12" cy="10" r="1.9" />
          <path d="M9 18.4 7 21m8-2.6 2 2.6" />
        </svg>
      );
    case "versions":
      return (
        <svg {...common}>
          <path d="M12 3.5 20 8l-8 4.5L4 8l8-4.5Z" />
          <path d="M4 12.2l8 4.5 8-4.5" />
          <path d="M4 16.4l8 4.5 8-4.5" />
        </svg>
      );
    case "plugins":
      return (
        <svg {...common}>
          <path d="M9.5 4.5a2 2 0 1 1 4 0V7h2.8a1.2 1.2 0 0 1 1.2 1.2v2.6h-1.9a2.1 2.1 0 0 0 0 4.2h1.9v2.8a1.2 1.2 0 0 1-1.2 1.2h-2.8v-1.9a2.1 2.1 0 0 0-4.2 0V19H6.5a1.2 1.2 0 0 1-1.2-1.2V8.2A1.2 1.2 0 0 1 6.5 7h3V4.5Z" />
        </svg>
      );
    case "env":
      return (
        <svg {...common}>
          <circle cx="12" cy="12" r="3.1" />
          <path d="M12 2.8v2.4M12 18.8v2.4M4.5 12H2.1m19.8 0h-2.4M6.7 6.7 5 5m14 14-1.7-1.7M17.3 6.7 19 5M5 19l1.7-1.7" />
        </svg>
      );
    case "console":
      return (
        <svg {...common}>
          <rect x="3" y="4.5" width="18" height="15" rx="2.2" />
          <path d="M7 9.5l2.6 2.5L7 14.5M12.4 15h4.2" />
        </svg>
      );
    case "settings":
      return (
        <svg {...common}>
          <path d="M4 7h10m3 0h3M4 12h4m3 0h9M4 17h13m3 0h0" />
          <circle cx="15.5" cy="7" r="2.1" />
          <circle cx="9.5" cy="12" r="2.1" />
          <circle cx="18.5" cy="17" r="2.1" />
        </svg>
      );
    case "sleep":
      return (
        <svg {...common}>
          <path d="M20.4 14.6A8.4 8.4 0 0 1 9.4 3.6a8.6 8.6 0 1 0 11 11Z" />
        </svg>
      );
    case "node":
      return (
        <svg {...common}>
          <path d="M12 3.2 19.5 7.4v9.2L12 20.8 4.5 16.6V7.4L12 3.2Z" />
          <path d="M9.6 14.6c.5.6 1.3 1 2.4 1 1.6 0 2.5-.8 2.5-2V8.6" />
        </svg>
      );
    case "harness":
      return (
        <svg {...common}>
          <path d="M5.5 8.5c3-3.4 8.2-4 11.8-1.3 1.4 1 1.9 2.6 1.2 4-.6 1.2-1.9 1.8-3.2 1.5" />
          <path d="M18.5 15.5c-3 3.4-8.2 4-11.8 1.3-1.4-1-1.9-2.6-1.2-4 .6-1.2 1.9-1.8 3.2-1.5" />
          <circle cx="12" cy="12" r="1.4" />
        </svg>
      );
    case "download":
      return (
        <svg {...common}>
          <path d="M12 4v9.4" />
          <path d="M8.2 10.2 12 14l3.8-3.8" />
          <path d="M4.8 16.6v1.6a1.8 1.8 0 0 0 1.8 1.8h10.8a1.8 1.8 0 0 0 1.8-1.8v-1.6" />
        </svg>
      );
    case "market":
      return (
        <svg {...common}>
          <path d="M4 8.5h16l-1.2 10a1.8 1.8 0 0 1-1.8 1.6H7a1.8 1.8 0 0 1-1.8-1.6L4 8.5Z" />
          <path d="M8.6 8.5V6.4a3.4 3.4 0 0 1 6.8 0v2.1" />
        </svg>
      );
    case "log":
      return (
        <svg {...common}>
          <path d="M6.5 3.5h8.6L19 7.4v13.1H6.5V3.5Z" />
          <path d="M14.6 3.6v4h4.2" />
          <path d="M9.4 11h6.2M9.4 14.2h6.2M9.4 17.4h3.8" />
        </svg>
      );
    case "play":
      return (
        <svg {...common}>
          <circle cx="12" cy="12" r="8.4" />
          <path d="M10.3 8.9l5 3.1-5 3.1V8.9Z" />
        </svg>
      );
    default:
      return null;
  }
}
