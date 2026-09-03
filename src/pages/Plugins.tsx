import { useCallback, useEffect, useState } from "react";
import { api, type MarketItem, type PluginInfo } from "../api";
import type { Shared } from "../App";
import { Busy, ConfirmModal, Empty, PromptModal } from "../ui";
import { Icon } from "../icons";

export default function PluginsPage(p: Shared) {
  const { profiles, selected, setSelected, toast } = p;
  const [installed, setInstalled] = useState<PluginInfo[]>([]);
  const [market, setMarket] = useState<MarketItem[]>([]);
  const [query, setQuery] = useState("");
  const [tab, setTab] = useState<"installed" | "market">("installed");
  const [busy, setBusy] = useState<string>("");
  const [marketLoaded, setMarketLoaded] = useState(false);
  const [dialog, setDialog] = useState<
    { kind: "manual" } | { kind: "remove"; name: string } | null
  >(null);

  const loadInstalled = useCallback(async () => {
    if (!selected) return setInstalled([]);
    try {
      setInstalled(await api.pluginList(selected));
    } catch (e) {
      toast.err(String(e));
      setInstalled([]);
    }
  }, [selected]);

  useEffect(() => {
    loadInstalled();
  }, [loadInstalled]);

  const loadMarket = useCallback(async () => {
    setBusy("正在拉取插件列表…");
    try {
      setMarket(await api.marketItems());
      setMarketLoaded(true);
    } catch (e) {
      toast.err(`拉取插件市场失败：${e}`);
    } finally {
      setBusy("");
    }
  }, []);

  const install = async (source: string) => {
    if (!selected) return toast.err("请先选择一个版本");
    setBusy(`正在安装 ${source}…`);
    p.setLogs((l) => [
      ...l,
      { profile: selected, stream: "system", line: `开始安装插件 ${source}`, ts: Date.now() },
    ]);
    try {
      await api.pluginAdd(selected, source);
      toast.ok(`已安装 ${source}`);
      await loadInstalled();
      await p.refreshProfiles();
      setDialog(null);
    } catch (e) {
      toast.err(`安装失败：${e}（详情见控制台）`);
    } finally {
      setBusy("");
    }
  };

  const remove = async (name: string) => {
    if (!selected) return;
    setBusy(`正在卸载 ${name}…`);
    try {
      await api.pluginRemove(selected, name);
      toast.ok(`已卸载 ${name}`);
      await loadInstalled();
      await p.refreshProfiles();
      setDialog(null);
    } catch (e) {
      toast.err(`卸载失败：${e}（详情见控制台）`);
    } finally {
      setBusy("");
    }
  };

  const q = query.trim().toLowerCase();
  const shownInstalled = q
    ? installed.filter((x) => x.name.toLowerCase().includes(q))
    : installed;
  const installedNames = new Set(installed.map((x) => x.name));
  const shownMarket = q
    ? market.filter(
        (x) =>
          x.name.toLowerCase().includes(q) || x.description.toLowerCase().includes(q)
      )
    : market;

  return (
    <div className="page">
      <div className="page-head">
        <h1 className="page-title">插件管理</h1>
        <p className="page-desc">
          harness 里一切能力都是插件。插件按版本隔离安装，改动会写进该版本的 package.json。
        </p>
      </div>

      <div className="card section">
        <div className="row row-wrap" style={{ gap: 10 }}>
          <label className="row" style={{ gap: 8 }}>
            <span className="field-label" style={{ margin: 0 }}>
              版本
            </span>
            <select
              className="select"
              style={{ width: 190 }}
              value={selected}
              onChange={(e) => setSelected(e.target.value)}
            >
              {profiles.length === 0 ? (
                <option value="">（无可用版本）</option>
              ) : (
                profiles.map((x) => (
                  <option key={x.name} value={x.name}>
                    {x.name}
                  </option>
                ))
              )}
            </select>
          </label>

          <div className="row grow" style={{ gap: 8 }}>
            <input
              className="input"
              placeholder="搜索插件名称或描述…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </div>

          <div className="segmented">
            <button
              className={tab === "installed" ? "btn btn-sm on" : "btn btn-sm"}
              onClick={() => setTab("installed")}
            >
              已安装 {installed.length} 个
            </button>
            <button
              className={tab === "market" ? "btn btn-sm on" : "btn btn-sm"}
              onClick={() => {
                setTab("market");
                if (!marketLoaded) loadMarket();
              }}
            >
              插件市场
            </button>
          </div>
          <span className="toolbar-sep" aria-hidden="true" />
          <button className="btn btn-sm" onClick={() => setDialog({ kind: "manual" })}>
            手动安装
          </button>
        </div>
        {busy ? (
          <div style={{ marginTop: 12 }}>
            <Busy text={busy} />
          </div>
        ) : null}
      </div>

      <div className="card page-body flush">
      {tab === "installed" ? (
        shownInstalled.length === 0 ? (
          <Empty
            icon="plugins"
            fill
            text={
              installed.length === 0
                ? "该版本还没安装任何插件。去「插件市场」挑一个，或用「手动安装」填 npm 包名 / github:owner/repo。"
                : "没有匹配的插件"
            }
          />
        ) : (
          <div className="list fill">
            {shownInstalled.map((x) => (
              <div key={x.name} className="list-row">
                <span className="icon-badge" aria-hidden="true">
                  <Icon name="plugins" />
                </span>
                <div className="list-main">
                  <div className="list-name">{x.name}</div>
                  <div className="list-sub mono">{x.version}</div>
                </div>
                <div className="list-actions">
                  <button
                    className="btn btn-sm btn-quiet-danger"
                    disabled={Boolean(busy)}
                    onClick={() => setDialog({ kind: "remove", name: x.name })}
                  >
                    卸载
                  </button>
                </div>
              </div>
            ))}
          </div>
        )
      ) : shownMarket.length === 0 ? (
        <Empty
          icon="market"
          fill
          text={
            marketLoaded
              ? "没有匹配的插件。市场来源为 npm 上带 dsh-plugin 关键词的包，可在设置里追加插件商店地址。"
              : "点击上方「插件市场」加载列表"
          }
        />
      ) : (
        <div className="list fill">
          {shownMarket.map((x) => {
            const already = installedNames.has(x.name);
            return (
              <div key={x.source} className="list-row">
                <span className="icon-badge" aria-hidden="true">
                  <Icon name={x.origin === "catalog" ? "market" : "download"} />
                </span>
                <div className="list-main">
                  <div className="list-name">
                    {x.name}
                    {x.version ? <span className="tag mono">v{x.version}</span> : null}
                    {already ? <span className="tag tag-ok">已安装</span> : null}
                  </div>
                  <div className="list-sub">{x.description || x.source}</div>
                </div>
                <div className="list-actions">
                  <button
                    className="btn btn-sm btn-primary"
                    disabled={Boolean(busy) || !selected}
                    onClick={() => install(x.source)}
                  >
                    {already ? "重新安装" : "安装"}
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}
      </div>

      {dialog?.kind === "manual" && (
        <PromptModal
          title="手动安装插件"
          desc="支持 npm 包名（如 @scope/dsh-plugin-foo）或 GitHub 源（github:owner/repo）。安装过程转发给官方 dsh plugin add 命令。"
          label="插件源"
          placeholder="@scope/dsh-plugin-foo 或 github:owner/repo"
          confirmText="安装插件"
          onClose={() => setDialog(null)}
          onConfirm={install}
        />
      )}
      {dialog?.kind === "remove" && (
        <ConfirmModal
          title={`卸载 ${dialog.name}？`}
          desc={`将从版本 ${selected} 中移除该插件及其依赖，配置层（cordis.patch.yml）里的相关条目需要你自行清理。`}
          confirmText="确认卸载"
          danger
          onClose={() => setDialog(null)}
          onConfirm={() => remove(dialog.name)}
        />
      )}
    </div>
  );
}
