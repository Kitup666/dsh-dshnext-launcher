import type { Shared } from "../App";
import { Empty } from "../ui";

function fmtUptime(s: number) {
  if (s < 60) return `${s} 秒`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} 分 ${s % 60} 秒`;
  return `${Math.floor(m / 60)} 时 ${m % 60} 分`;
}

export default function HomePage(p: Shared) {
  const { env, profiles, procs, selected, setSelected, setPage, runningNames, config } = p;
  const running = procs.find((x) => x.profile === selected);
  const envReady = Boolean(env?.dsh_version);
  const current = profiles.find((x) => x.name === selected);
  const pluginCount = current ? Object.keys(current.dependencies).length : 0;

  return (
    <div className="page">
      <div className="hero">
        <div className="hero-grid">
          <div>
            <div className="hero-eyebrow">DeepSeek Harness</div>
            <h1 className="hero-title">{selected || "尚无版本"}</h1>
            <p className="hero-desc">
              {envReady
                ? "选择一个版本，一键启动 harness 的 Web 界面。"
                : "还没检测到 dsh，请先到「环境」页完成安装。"}
            </p>
          </div>

          <div className="hero-actions">
            <select
              className="select"
              value={selected}
              onChange={(e) => setSelected(e.target.value)}
              disabled={profiles.length === 0}
            >
              {profiles.length === 0 ? (
                <option value="">（无可用版本）</option>
              ) : (
                profiles.map((x) => (
                  <option key={x.name} value={x.name}>
                    {x.name}
                    {runningNames.has(x.name) ? " · 运行中" : ""}
                  </option>
                ))
              )}
            </select>

            {running ? (
              <>
                <button className="btn btn-hero btn-teal" onClick={() => p.stop(selected)}>
                  停止运行
                </button>
                <button className="btn btn-hero btn-ghost" onClick={() => p.openUi(selected)}>
                  打开界面
                </button>
              </>
            ) : (
              <button
                className="btn btn-hero btn-primary"
                disabled={!envReady || !selected}
                onClick={() => p.start(selected)}
              >
                启动 harness
              </button>
            )}
          </div>
        </div>

        <div className="meta-strip">
          <div className="meta">
            <span className="meta-label">状态</span>
            <span className="meta-value">
              {running ? (
                <span className="row" style={{ gap: 7 }}>
                  <span className="dot dot-live" />
                  运行中
                </span>
              ) : (
                "空闲"
              )}
            </span>
          </div>
          <div className="meta">
            <span className="meta-label">运行时长</span>
            <span className="meta-value">{running ? fmtUptime(running.uptime_secs) : "—"}</span>
          </div>
          <div className="meta">
            <span className="meta-label">Web 地址</span>
            <span className="meta-value mono">
              {running ? running.url.replace(/^https?:\/\//, "") : `127.0.0.1:${config?.port ?? 3080}`}
            </span>
          </div>
          <div className="meta">
            <span className="meta-label">进程 PID</span>
            <span className="meta-value">{running?.pid ?? "—"}</span>
          </div>
          <div className="meta">
            <span className="meta-label">插件</span>
            <span className="meta-value">{pluginCount} 个</span>
          </div>
        </div>
      </div>

      <div className="section card page-body flush">
        <div style={{ padding: "0 26px" }}>
          <h2 className="card-title">运行中的实例</h2>
          <p className="card-sub">每个版本独立进程，可同时运行多个（注意端口不要冲突）。</p>
        </div>
        {procs.length === 0 ? (
          <Empty icon="sleep" text="当前没有运行中的实例" fill />
        ) : (
          <div className="list fill">
            {procs.map((x) => (
              <div key={x.profile} className="list-row">
                <span className="icon-badge" aria-hidden="true">
                  <span className="dot dot-live" />
                </span>
                <div className="list-main">
                  <div className="list-name">{x.profile}</div>
                  <div className="list-sub mono">
                    {x.url} · PID {x.pid} · {fmtUptime(x.uptime_secs)}
                  </div>
                </div>
                <div className="list-actions">
                  <button className="btn btn-sm" onClick={() => p.openUi(x.profile)}>
                    打开界面
                  </button>
                  <button className="btn btn-sm" onClick={() => setPage("console")}>
                    看日志
                  </button>
                  <button className="btn btn-sm btn-quiet-danger" onClick={() => p.stop(x.profile)}>
                    停止
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
