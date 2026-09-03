import { useCallback, useEffect, useState } from "react";
import { api, type NodeRelease } from "../api";
import type { Shared } from "../App";
import { Busy, ConfirmModal, PathText } from "../ui";
import { Icon } from "../icons";

export default function EnvPage(p: Shared) {
  const { env, toast, refreshEnv } = p;
  const [busy, setBusy] = useState("");
  const [includeRc, setIncludeRc] = useState(true);
  const [versions, setVersions] = useState<string[]>([]);
  const [latest, setLatest] = useState<string | null>(null);
  const [pick, setPick] = useState("latest");
  const [nodeVersion, setNodeVersion] = useState("24.19.0");
  const [nodeList, setNodeList] = useState<NodeRelease[]>([]);
  const [dialog, setDialog] = useState<"node" | "dsh" | null>(null);

  const loadVersions = useCallback(async () => {
    setBusy("正在查询可用版本…");
    try {
      const [v, nodes] = await Promise.all([
        api.dshVersions(includeRc),
        api.nodeVersions().catch(() => [] as NodeRelease[]),
      ]);
      setVersions(v.versions);
      setLatest(v.latest);
      setNodeList(nodes);
      if (nodes.length > 0 && !nodes.some((n) => n.version === nodeVersion)) {
        setNodeVersion(nodes.find((n) => n.lts)?.version ?? nodes[0].version);
      }
    } catch (e) {
      toast.err(`查询版本失败：${e}`);
    } finally {
      setBusy("");
    }
  }, [includeRc]);

  useEffect(() => {
    loadVersions();
  }, [loadVersions]);

  const run = async (label: string, fn: () => Promise<void>, okText: string) => {
    setBusy(label);
    p.setLogs((l) => [...l, { profile: "env", stream: "system", line: label, ts: Date.now() }]);
    try {
      await fn();
      await refreshEnv();
      toast.ok(okText);
    } catch (e) {
      toast.err(`${label}失败：${e}（详情见控制台）`);
    } finally {
      setBusy("");
    }
  };

  const nodeOk = Boolean(env?.node_version);
  const dshOk = Boolean(env?.dsh_version);
  const pnpmOk = Boolean(env?.pnpm_version);

  return (
    <div className="page">
      <div className="page-head row" style={{ justifyContent: "space-between" }}>
        <div>
          <h1 className="page-title">环境</h1>
          <p className="page-desc">
            启动器在私有目录里托管运行时，与系统里的 Node / dsh 互不干扰。
          </p>
        </div>
        <button className="btn" disabled={Boolean(busy)} onClick={() => refreshEnv()}>
          重新检测
        </button>
      </div>

      {busy ? (
        <div className="card section">
          <Busy text={busy} />
          <p className="field-hint" style={{ marginTop: 8 }}>
            安装过程的完整输出会实时写入「控制台」页。
          </p>
        </div>
      ) : null}

      <div className="card section">
        <h2 className="card-title">检测结果</h2>
        <p className="card-sub">三项齐全才能启动 harness 并管理插件。</p>
        <div className="list">
          <div className="list-row">
            <span className="icon-badge" aria-hidden="true">
              <Icon name="node" />
            </span>
            <div className="list-main">
              <div className="list-name">
                Node.js
                <span className={`tag ${nodeOk ? "tag-ok" : "tag-bad"}`}>
                  {env?.node_version ?? "未安装"}
                </span>
                {env?.node_managed ? <span className="tag tag-accent">启动器托管</span> : null}
              </div>
              <div className="list-sub wrap mono">
                {env?.node_path ? <PathText path={env.node_path} /> : "未找到可用的 node"}
              </div>
            </div>
            <div className="list-actions">
              <select
                className="select"
                style={{ width: 168 }}
                value={nodeVersion}
                onChange={(e) => setNodeVersion(e.target.value)}
                aria-label="Node 版本"
              >
                {nodeList.length === 0 ? (
                  <option value={nodeVersion}>{nodeVersion}</option>
                ) : (
                  nodeList.map((n) => (
                    <option key={n.version} value={n.version}>
                      {n.version}
                      {n.lts ? " · LTS" : ""}
                    </option>
                  ))
                )}
              </select>
              {env?.node_managed ? (
                <button
                  className="btn btn-sm btn-quiet-danger"
                  disabled={Boolean(busy)}
                  onClick={() => setDialog("node")}
                >
                  删除托管版
                </button>
              ) : (
                <button
                  className="btn btn-sm btn-primary"
                  disabled={Boolean(busy)}
                  onClick={() =>
                    run("正在下载安装便携版 Node.js", () => api.installNode(nodeVersion), "Node.js 安装完成")
                  }
                >
                  下载安装
                </button>
              )}
            </div>
          </div>

          <div className="list-row">
            <span className="icon-badge" aria-hidden="true">
              <Icon name="harness" />
            </span>
            <div className="list-main">
              <div className="list-name">
                DeepSeek Harness (dsh)
                <span className={`tag ${dshOk ? "tag-ok" : "tag-bad"}`}>
                  {env?.dsh_version ?? "未安装"}
                </span>
                {latest && env?.dsh_version && latest !== env.dsh_version ? (
                  <span className="tag tag-warn">可更新到 {latest}</span>
                ) : null}
              </div>
              <div className="list-sub wrap mono">
                {env?.dsh_path ? <PathText path={env.dsh_path} /> : "未安装到启动器目录"}
              </div>
            </div>
            <div className="list-actions">
              <select
                className="select"
                style={{ width: 168 }}
                value={pick}
                onChange={(e) => setPick(e.target.value)}
                aria-label="dsh 版本"
              >
                <option value="latest">latest{latest ? ` (${latest})` : ""}</option>
                {versions.map((v) => (
                  <option key={v} value={v}>
                    {v}
                  </option>
                ))}
              </select>
              <button
                className="btn btn-sm btn-primary"
                disabled={Boolean(busy) || !nodeOk}
                onClick={() =>
                  run(
                    `正在安装 dsh ${pick}`,
                    () => api.installDsh(pick),
                    `dsh ${pick} 安装完成`
                  )
                }
              >
                {dshOk ? "安装 / 切换" : "安装"}
              </button>
              {dshOk ? (
                <button
                  className="btn btn-sm btn-quiet-danger"
                  disabled={Boolean(busy)}
                  onClick={() => setDialog("dsh")}
                >
                  卸载
                </button>
              ) : null}
            </div>
          </div>

          <div className="list-row">
            <span className="icon-badge" aria-hidden="true">
              <Icon name="download" />
            </span>
            <div className="list-main">
              <div className="list-name">
                pnpm
                <span className={`tag ${pnpmOk ? "tag-ok" : "tag-warn"}`}>
                  {env?.pnpm_version ?? "未安装"}
                </span>
              </div>
              <div className="list-sub">
                官方 dsh 的插件管理命令依赖 pnpm；缺失时插件安装/卸载会失败。
              </div>
            </div>
            <div className="list-actions">
              <button
                className="btn btn-sm btn-primary"
                disabled={Boolean(busy) || !nodeOk}
                onClick={() => run("正在安装 pnpm", () => api.installPnpm(), "pnpm 安装完成")}
              >
                {pnpmOk ? "重新安装" : "安装"}
              </button>
            </div>
          </div>
        </div>

        <div className="strip">
          <label className="checkbox">
            <input
              type="checkbox"
              checked={includeRc}
              onChange={(e) => setIncludeRc(e.target.checked)}
            />
            显示预发布版本（rc / beta）
          </label>
          <button className="btn btn-sm" disabled={Boolean(busy)} onClick={loadVersions}>
            刷新版本列表
          </button>
        </div>
      </div>

      <div className="card section">
        <div className="row" style={{ justifyContent: "space-between", alignItems: "flex-start" }}>
          <div>
            <h2 className="card-title">数据目录</h2>
            <p className="card-sub" style={{ marginBottom: 12 }}>
              配置、运行时和所有版本都存放在这里。
            </p>
          </div>
          <button
            className="btn btn-sm"
            onClick={() => api.openDataDir().catch((e) => toast.err(String(e)))}
          >
            在资源管理器中打开
          </button>
        </div>
        <div className="kv">
          <span className="kv-key">启动器目录</span>
          <span className="kv-val mono">
            {env ? <PathText path={env.data_dir} /> : "—"}
          </span>
        </div>
        <div className="kv">
          <span className="kv-key">DSH_HOME</span>
          <span className="kv-val mono">{env ? <PathText path={env.home_dir} /> : "—"}</span>
        </div>
      </div>

      {dialog === "node" && (
        <ConfirmModal
          title="删除托管的 Node.js？"
          desc="删除后需要重新下载才能启动 harness（不影响你系统里自己安装的 Node）。"
          confirmText="删除"
          danger
          onClose={() => setDialog(null)}
          onConfirm={() => {
            setDialog(null);
            run("正在删除托管 Node.js", () => api.uninstallNode(), "已删除托管 Node.js");
          }}
        />
      )}
      {dialog === "dsh" && (
        <ConfirmModal
          title="卸载 dsh？"
          desc="仅删除启动器目录下的 dsh 及其依赖，你创建的版本（profile）和会话记录会保留。"
          confirmText="卸载"
          danger
          onClose={() => setDialog(null)}
          onConfirm={() => {
            setDialog(null);
            run("正在卸载 dsh", () => api.uninstallDsh(), "已卸载 dsh");
          }}
        />
      )}
    </div>
  );
}
