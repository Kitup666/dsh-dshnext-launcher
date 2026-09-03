import { useEffect, useState } from "react";
import { api, type Config } from "../api";
import type { Shared } from "../App";

const NODE_MIRRORS = [
  { label: "npmmirror（国内推荐）", value: "https://npmmirror.com/mirrors/node" },
  { label: "nodejs.org 官方", value: "" },
];
const NPM_REGISTRIES = [
  { label: "npmjs.org 官方", value: "" },
  { label: "npmmirror（国内推荐）", value: "https://registry.npmmirror.com" },
];

export default function SettingsPage(p: Shared) {
  const { config, toast, saveConfig } = p;
  const [draft, setDraft] = useState<Config | null>(config);
  const [showKey, setShowKey] = useState(false);

  useEffect(() => {
    setDraft(config);
  }, [config]);

  if (!draft) return <div className="page">加载中…</div>;

  const set = <K extends keyof Config>(k: K, v: Config[K]) =>
    setDraft({ ...draft, [k]: v });

  const dirty = JSON.stringify(draft) !== JSON.stringify(config);

  const applyTheme = async (theme: string) => {
    if (theme === draft.theme) return;
    const next = { ...draft, theme };
    setDraft(next);
    try {
      await saveConfig(next); // 主题立即生效并落盘，不影响其它未保存项
    } catch (e) {
      toast.err(`保存主题失败：${e}`);
    }
  };

  const save = async () => {
    try {
      await saveConfig(draft);
      toast.ok("设置已保存");
    } catch (e) {
      toast.err(`保存失败：${e}`);
    }
  };

  return (
    <div className="page">
      <div className="page-head row" style={{ justifyContent: "space-between" }}>
        <div>
          <h1 className="page-title">设置</h1>
          <p className="page-desc">
            设置保存在启动器数据目录的 config.json 里；API Key 仅在启动时注入子进程环境变量。
          </p>
        </div>
        <div className="row">
          {dirty ? <span className="tag tag-warn">有未保存的修改</span> : null}
          <button className="btn btn-primary" onClick={save} disabled={!dirty}>
            保存
          </button>
        </div>
      </div>

      <div className="card section">
        <h2 className="card-title">外观</h2>
        <p className="card-sub">切换后立即生效。浅色为默认；暗色是纯黑工作台风。</p>
        <div className="segmented">
          <button
            className={draft.theme !== "dark" ? "btn btn-sm on" : "btn btn-sm"}
            onClick={() => applyTheme("light")}
          >
            浅色
          </button>
          <button
            className={draft.theme === "dark" ? "btn btn-sm on" : "btn btn-sm"}
            onClick={() => applyTheme("dark")}
          >
            暗色
          </button>
        </div>
      </div>

      <div className="card section">
        <h2 className="card-title">模型访问</h2>
        <p className="card-sub">
          harness 需要 DeepSeek API Key 才能调用模型。留空则由 harness 自身的配置或环境变量决定。
        </p>
        <label className="field">
          <span className="field-label">DEEPSEEK_API_KEY</span>
          <div className="row">
            <input
              className="input mono grow"
              type={showKey ? "text" : "password"}
              value={draft.api_key}
              placeholder="sk-…"
              autoComplete="off"
              onChange={(e) => set("api_key", e.target.value)}
            />
            <button className="btn btn-sm" onClick={() => setShowKey((v) => !v)}>
              {showKey ? "隐藏" : "显示"}
            </button>
          </div>
          <div className="field-hint">
            以明文保存在本机 config.json 中，仅作为环境变量传给 dsh 子进程，不会上传到任何地方。
          </div>
        </label>
      </div>

      <div className="card section">
        <h2 className="card-title">启动行为</h2>
        <p className="card-sub">控制 harness 的监听端口与 Web 界面的打开方式。</p>
        <label className="field">
          <span className="field-label">默认端口</span>
          <input
            className="input mono"
            style={{ width: 130 }}
            type="number"
            min={1}
            max={65535}
            value={draft.port}
            onChange={(e) => set("port", Number(e.target.value) || 3080)}
          />
          <div className="field-hint">
            同时运行多个版本时端口会冲突，届时改这里再启动下一个。
          </div>
        </label>
        <label className="field">
          <span className="field-label">Web 界面打开方式</span>
          <select
            className="select"
            style={{ width: 240 }}
            value={draft.open_mode}
            onChange={(e) => set("open_mode", e.target.value)}
          >
            <option value="window">启动器内置窗口</option>
            <option value="browser">系统默认浏览器</option>
          </select>
        </label>
        <label className="checkbox">
          <input
            type="checkbox"
            checked={draft.auto_open}
            onChange={(e) => set("auto_open", e.target.checked)}
          />
          启动成功后自动打开 Web 界面
        </label>
      </div>

      <div className="card section">
        <h2 className="card-title">下载源</h2>
        <p className="card-sub">国内网络建议使用镜像，能显著加快 Node 与插件的下载。</p>
        <label className="field">
          <span className="field-label">Node.js 下载镜像</span>
          <select
            className="select"
            style={{ width: 300 }}
            value={NODE_MIRRORS.some((m) => m.value === draft.node_mirror) ? draft.node_mirror : "__custom"}
            onChange={(e) => e.target.value !== "__custom" && set("node_mirror", e.target.value)}
          >
            {NODE_MIRRORS.map((m) => (
              <option key={m.label} value={m.value}>
                {m.label}
              </option>
            ))}
            <option value="__custom">自定义…</option>
          </select>
          <input
            className="input mono"
            style={{ marginTop: 8 }}
            value={draft.node_mirror}
            placeholder="https://nodejs.org/dist"
            onChange={(e) => set("node_mirror", e.target.value)}
          />
        </label>
        <label className="field">
          <span className="field-label">npm registry</span>
          <select
            className="select"
            style={{ width: 300 }}
            value={NPM_REGISTRIES.some((m) => m.value === draft.npm_registry) ? draft.npm_registry : "__custom"}
            onChange={(e) => e.target.value !== "__custom" && set("npm_registry", e.target.value)}
          >
            {NPM_REGISTRIES.map((m) => (
              <option key={m.label} value={m.value}>
                {m.label}
              </option>
            ))}
            <option value="__custom">自定义…</option>
          </select>
          <input
            className="input mono"
            style={{ marginTop: 8 }}
            value={draft.npm_registry}
            placeholder="https://registry.npmjs.org"
            onChange={(e) => set("npm_registry", e.target.value)}
          />
        </label>
        <label className="field">
          <span className="field-label">插件商店目录（catalog.json）</span>
          <input
            className="input mono"
            value={draft.plugin_catalog_url}
            placeholder="留空则只使用 npm 上的 dsh-plugin 关键词作为来源"
            onChange={(e) => set("plugin_catalog_url", e.target.value)}
          />
          <div className="field-hint">
            填入第三方插件目录的 JSON 地址后，插件市场会把它和 npm 上带 dsh-plugin 关键词的包合并展示。
          </div>
        </label>
      </div>

      <div className="card section">
        <h2 className="card-title">关于</h2>
        <p className="card-sub">
          DshDesk 是 DeepSeek Harness 的第三方 Windows 启动器，负责运行时托管、版本隔离与插件管理。
          harness 本体是 DeepSeek 开源的 MIT 项目。
        </p>
        <div className="row row-wrap">
          <button
            className="btn btn-sm"
            onClick={() => api.openInBrowser("https://www.deepseek.com/harness/").catch(() => {})}
          >
            DeepSeek Harness 官网
          </button>
          <button
            className="btn btn-sm"
            onClick={() =>
              api.openInBrowser("https://github.com/deepseek-ai/deepseek-harness").catch(() => {})
            }
          >
            GitHub 仓库
          </button>
          <button
            className="btn btn-sm"
            onClick={() => api.openInBrowser("https://github.com/topics/dsh-plugin").catch(() => {})}
          >
            社区插件
          </button>
        </div>
      </div>
    </div>
  );
}
