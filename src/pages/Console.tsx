import { useEffect, useMemo, useRef, useState } from "react";
import type { Shared } from "../App";
import { Empty } from "../ui";

function fmtTime(ts: number) {
  if (!ts) return "";
  const d = new Date(ts);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

export default function ConsolePage(p: Shared) {
  const { logs, profiles, toast } = p;
  const [filter, setFilter] = useState("all");
  const [autoScroll, setAutoScroll] = useState(true);
  const boxRef = useRef<HTMLDivElement>(null);

  const sources = useMemo(() => {
    const set = new Set<string>(logs.map((l) => l.profile));
    profiles.forEach((x) => set.add(x.name));
    return Array.from(set).sort();
  }, [logs, profiles]);

  const shown = filter === "all" ? logs : logs.filter((l) => l.profile === filter);

  useEffect(() => {
    if (autoScroll && boxRef.current) {
      boxRef.current.scrollTop = boxRef.current.scrollHeight;
    }
  }, [shown.length, autoScroll]);

  const copyAll = async () => {
    const text = shown.map((l) => `[${fmtTime(l.ts)}] [${l.profile}] ${l.line}`).join("\n");
    try {
      await navigator.clipboard.writeText(text);
      toast.ok(`已复制 ${shown.length} 行日志`);
    } catch (e) {
      toast.err(`复制失败：${e}`);
    }
  };

  return (
    <div className="page page-fixed">
      <div className="page-head">
        <h1 className="page-title">控制台</h1>
        <p className="page-desc">
          harness 进程、插件安装和环境安装的实时输出（最多保留 2000 行）。
        </p>
      </div>

      <div className="card page-body">
        <div className="console-head row row-wrap">
        <select
          className="select"
          style={{ width: 190 }}
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          aria-label="日志来源"
        >
          <option value="all">全部来源</option>
          {sources.map((s) => (
            <option key={s} value={s}>
              {s}
            </option>
          ))}
        </select>
        <label className="checkbox">
          <input
            type="checkbox"
            checked={autoScroll}
            onChange={(e) => setAutoScroll(e.target.checked)}
          />
          自动滚动
        </label>
        <span className="tag">{shown.length} 行</span>
        <div className="grow" />
        <button className="btn btn-sm" onClick={copyAll} disabled={shown.length === 0}>
          复制
        </button>
        <button className="btn btn-sm" onClick={() => p.setLogs([])} disabled={logs.length === 0}>
          清空
        </button>
      </div>

      {shown.length === 0 ? (
        <Empty icon="log" text="暂无输出。启动 harness 或安装插件后，日志会实时出现在这里。" fill />
      ) : (
        <div className="console" ref={boxRef}>
          {shown.map((l, i) => (
            <span key={i} className={`log-line log-${l.stream}`}>
              <span className="log-ts">{fmtTime(l.ts)}</span>
              {filter === "all" ? <span className="log-ts">[{l.profile}]</span> : null}
              {l.line}
            </span>
          ))}
        </div>
      )}
      </div>
    </div>
  );
}
