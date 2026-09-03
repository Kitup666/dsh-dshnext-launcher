import { useEffect, useState, type ReactNode } from "react";
import { Icon } from "./icons";
import type { Toast } from "./useToasts";

export function ToastHost({ toasts }: { toasts: Toast[] }) {
  const icon = { ok: "✓", err: "✕", info: "i" };
  return (
    <div className="toasts">
      {toasts.map((t) => (
        <div key={t.id} className={`toast toast-${t.kind}`}>
          <span aria-hidden="true">{icon[t.kind]}</span>
          <span>{t.text}</span>
        </div>
      ))}
    </div>
  );
}

/* ===== Modal ===== */
export function Modal({
  title,
  desc,
  children,
  onClose,
  footer,
}: {
  title: string;
  desc?: string;
  children?: ReactNode;
  onClose: () => void;
  footer: ReactNode;
}) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  return (
    <div className="modal-mask" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal" role="dialog" aria-modal="true" aria-label={title}>
        <h3 className="modal-title">{title}</h3>
        {desc ? <p className="modal-desc">{desc}</p> : null}
        {children}
        <div className="modal-foot">{footer}</div>
      </div>
    </div>
  );
}

/* 单行输入型对话框：新建 / 重命名 / 复制 / 手动装插件 */
export function PromptModal({
  title,
  desc,
  label,
  placeholder,
  initial,
  confirmText,
  onConfirm,
  onClose,
}: {
  title: string;
  desc?: string;
  label: string;
  placeholder?: string;
  initial?: string;
  confirmText: string;
  onConfirm: (value: string) => void;
  onClose: () => void;
}) {
  const [value, setValue] = useState(initial ?? "");
  const submit = () => {
    if (value.trim()) onConfirm(value.trim());
  };
  return (
    <Modal
      title={title}
      desc={desc}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            取消
          </button>
          <button className="btn btn-primary" onClick={submit} disabled={!value.trim()}>
            {confirmText}
          </button>
        </>
      }
    >
      <label className="field">
        <span className="field-label">{label}</span>
        <input
          className="input"
          autoFocus
          value={value}
          placeholder={placeholder}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && submit()}
        />
      </label>
    </Modal>
  );
}

export function ConfirmModal({
  title,
  desc,
  confirmText,
  danger,
  onConfirm,
  onClose,
}: {
  title: string;
  desc: string;
  confirmText: string;
  danger?: boolean;
  onConfirm: () => void;
  onClose: () => void;
}) {
  return (
    <Modal
      title={title}
      desc={desc}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            取消
          </button>
          <button className={danger ? "btn btn-danger" : "btn btn-primary"} onClick={onConfirm}>
            {confirmText}
          </button>
        </>
      }
    />
  );
}

export function Empty({ icon, text, fill }: { icon: string; text: string; fill?: boolean }) {
  return (
    <div className={fill ? "empty fill" : "empty"}>
      <span className="empty-icon" aria-hidden="true">
        <Icon name={icon} size={34} />
      </span>
      {text}
    </div>
  );
}

export function Busy({ text }: { text: string }) {
  return (
    <span className="row" style={{ gap: 8, color: "var(--ink-dim)", fontSize: "0.84rem" }}>
      <span className="spinner" aria-hidden="true" />
      {text}
    </span>
  );
}

/** 长路径：在每个反斜杠后给出换行机会，避免折行时把文件名劈成孤字 */
export function PathText({ path, className }: { path: string; className?: string }) {
  const parts = path.split("\\");
  return (
    <span className={className}>
      {parts.map((seg, i) => (
        <span key={i}>
          {i > 0 ? "\\" : ""}
          {seg}
          {i < parts.length - 1 ? <wbr /> : null}
        </span>
      ))}
    </span>
  );
}
