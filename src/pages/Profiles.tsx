import { useState } from "react";
import { api } from "../api";
import type { Shared } from "../App";
import { ConfirmModal, Empty, PromptModal } from "../ui";
import { Icon } from "../icons";

type Dialog =
  | { kind: "create" }
  | { kind: "rename"; name: string }
  | { kind: "copy"; name: string }
  | { kind: "delete"; name: string }
  | null;

export default function ProfilesPage(p: Shared) {
  const { profiles, selected, setSelected, runningNames, toast, refreshProfiles } = p;
  const [dialog, setDialog] = useState<Dialog>(null);

  const wrap = async (fn: () => Promise<void>, okText: string) => {
    try {
      await fn();
      await refreshProfiles();
      toast.ok(okText);
      setDialog(null);
    } catch (e) {
      toast.err(String(e));
    }
  };

  return (
    <div className="page">
      <div className="page-head row" style={{ justifyContent: "space-between" }}>
        <div>
          <h1 className="page-title">版本管理</h1>
          <p className="page-desc">
            每个版本对应一个 harness profile（独立插件集与配置层），互不影响。
          </p>
        </div>
        <button className="btn btn-primary" onClick={() => setDialog({ kind: "create" })}>
          + 新建版本
        </button>
      </div>

      <div className="card page-body flush">
        {profiles.length === 0 ? (
          <Empty icon="versions" text="还没有版本。点右上角「新建版本」创建第一个。" fill />
        ) : (
          <div className="list fill">
          {profiles.map((x) => {
            const isRunning = runningNames.has(x.name);
            const pluginCount = Object.keys(x.dependencies).length;
            return (
              <div
                key={x.name}
                className={`list-row${selected === x.name ? " selected" : ""}`}
                onClick={() => setSelected(x.name)}
              >
                <span className="icon-badge" aria-hidden="true">
                  <Icon name="versions" />
                </span>
                <div className="list-main">
                  <div className="list-name">
                    {x.name}
                    {isRunning ? (
                      <span className="tag tag-ok">
                        <span className="dot dot-live" aria-hidden="true" />
                        运行中
                      </span>
                    ) : null}
                    {selected === x.name ? <span className="tag tag-accent">当前</span> : null}
                    <span className="tag">{pluginCount} 个插件</span>
                  </div>
                  <div className="list-sub wrap mono">
                    {x.bundles.length ? x.bundles.join(" + ") : "未声明 bundle"}
                  </div>
                </div>
                <div className="list-actions">
                  {isRunning ? (
                    <button
                      className="btn btn-sm btn-danger"
                      onClick={(e) => {
                        e.stopPropagation();
                        p.stop(x.name);
                      }}
                    >
                      停止
                    </button>
                  ) : (
                    <button
                      className="btn btn-sm btn-primary"
                      disabled={!p.env?.dsh_version}
                      onClick={(e) => {
                        e.stopPropagation();
                        p.start(x.name);
                      }}
                    >
                      启动
                    </button>
                  )}
                  <button
                    className="btn btn-sm"
                    onClick={(e) => {
                      e.stopPropagation();
                      setSelected(x.name);
                      p.setPage("plugins");
                    }}
                  >
                    插件
                  </button>
                  <button
                    className="btn btn-sm"
                    onClick={(e) => {
                      e.stopPropagation();
                      setDialog({ kind: "copy", name: x.name });
                    }}
                  >
                    复制
                  </button>
                  <button
                    className="btn btn-sm"
                    onClick={(e) => {
                      e.stopPropagation();
                      setDialog({ kind: "rename", name: x.name });
                    }}
                  >
                    重命名
                  </button>
                  <button
                    className="btn btn-sm"
                    onClick={(e) => {
                      e.stopPropagation();
                      api.openProfileDir(x.name).catch((err) => toast.err(String(err)));
                    }}
                  >
                    打开目录
                  </button>
                  <span className="toolbar-sep" aria-hidden="true" />
                  <button
                    className="btn btn-sm btn-quiet-danger"
                    onClick={(e) => {
                      e.stopPropagation();
                      setDialog({ kind: "delete", name: x.name });
                    }}
                  >
                    删除
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}
      </div>

      {dialog?.kind === "create" && (
        <PromptModal
          title="新建版本"
          desc="将在 harness 数据目录下创建一个新的 profile（Web 应用模板）。"
          label="版本名称（英文、数字、- 、_）"
          placeholder="例如 my-agent"
          confirmText="创建"
          onClose={() => setDialog(null)}
          onConfirm={(v) => wrap(() => api.profileCreate(v), `已创建版本 ${v}`)}
        />
      )}
      {dialog?.kind === "rename" && (
        <PromptModal
          title={`重命名 ${dialog.name}`}
          label="新名称"
          initial={dialog.name}
          confirmText="重命名"
          onClose={() => setDialog(null)}
          onConfirm={(v) => wrap(() => api.profileRename(dialog.name, v), `已重命名为 ${v}`)}
        />
      )}
      {dialog?.kind === "copy" && (
        <PromptModal
          title={`复制 ${dialog.name}`}
          desc="复制配置与插件清单（不含已安装的依赖，首次启动会自动装回）。"
          label="新版本名称"
          initial={`${dialog.name}-copy`}
          confirmText="复制"
          onClose={() => setDialog(null)}
          onConfirm={(v) => wrap(() => api.profileCopy(dialog.name, v), `已复制为 ${v}`)}
        />
      )}
      {dialog?.kind === "delete" && (
        <ConfirmModal
          title={`删除版本 ${dialog.name}？`}
          desc="该版本的配置、插件清单和已安装依赖将被永久删除，无法恢复。会话记录若存放在该目录下也会一并删除。"
          confirmText="永久删除"
          danger
          onClose={() => setDialog(null)}
          onConfirm={() => wrap(() => api.profileDelete(dialog.name), `已删除 ${dialog.name}`)}
        />
      )}
    </div>
  );
}
