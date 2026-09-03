import { useState } from "react";

export type ToastKind = "ok" | "err" | "info";
export interface Toast {
  id: number;
  kind: ToastKind;
  text: string;
}

let toastSeq = 1;

/** 独立成文件：与组件分开导出，React Fast Refresh 才能正确处理热更新 */
export function useToasts() {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const push = (kind: ToastKind, text: string) => {
    const id = toastSeq++;
    setToasts((t) => [...t, { id, kind, text }]);
    window.setTimeout(
      () => setToasts((t) => t.filter((x) => x.id !== id)),
      kind === "err" ? 7000 : 3600
    );
  };
  return {
    toasts,
    ok: (t: string) => push("ok", t),
    err: (t: unknown) => push("err", typeof t === "string" ? t : String(t)),
    info: (t: string) => push("info", t),
  };
}
