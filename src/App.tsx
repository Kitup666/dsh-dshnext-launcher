import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  api,
  onEvent,
  type Config,
  type EnvStatus,
  type LogLine,
  type ProcStatus,
  type ProfileInfo,
} from "./api";
import { ToastHost } from "./ui";
import { useToasts } from "./useToasts";
import { Icon } from "./icons";
import HomePage from "./pages/Home";
import ProfilesPage from "./pages/Profiles";
import PluginsPage from "./pages/Plugins";
import EnvPage from "./pages/Env";
import SettingsPage from "./pages/Settings";
import ConsolePage from "./pages/Console";

export type PageId = "home" | "profiles" | "plugins" | "env" | "settings" | "console";

const NAV: { id: PageId; icon: string; label: string }[] = [
  { id: "home", icon: "launch", label: "启动" },
  { id: "profiles", icon: "versions", label: "版本管理" },
  { id: "plugins", icon: "plugins", label: "插件管理" },
  { id: "env", icon: "env", label: "环境" },
  { id: "console", icon: "console", label: "控制台" },
  { id: "settings", icon: "settings", label: "设置" },
];

const MAX_LOGS = 2000;

export default function App() {
  const toast = useToasts();
  const [page, setPage] = useState<PageId>("home");
  const [config, setConfig] = useState<Config | null>(null);
  const [env, setEnv] = useState<EnvStatus | null>(null);
  const [profiles, setProfiles] = useState<ProfileInfo[]>([]);
  const [procs, setProcs] = useState<ProcStatus[]>([]);
  const [logs, setLogs] = useState<LogLine[]>([]);
  const [selected, setSelected] = useState<string>("");
  const [urls, setUrls] = useState<Record<string, string>>({});
  const openedRef = useRef<Set<string>>(new Set());
  const cfgRef = useRef<Config | null>(null);
  cfgRef.current = config;

  const appendLog = useCallback((l: LogLine) => {
    setLogs((prev) => {
      const next = prev.length >= MAX_LOGS ? prev.slice(prev.length - MAX_LOGS + 1) : prev.slice();
      next.push(l);
      return next;
    });
  }, []);

  const refreshProfiles = useCallback(async () => {
    try {
      const list = await api.profileList();
      setProfiles(list);
      setSelected((cur) => (cur && list.some((p) => p.name === cur) ? cur : list[0]?.name ?? ""));
    } catch (e) {
      toast.err(`读取版本列表失败：${e}`);
    }
  }, []);

  const refreshEnv = useCallback(async () => {
    try {
      setEnv(await api.envStatus());
    } catch (e) {
      toast.err(`检测环境失败：${e}`);
    }
  }, []);

  const refreshProcs = useCallback(async () => {
    try {
      setProcs(await api.dshStatus());
    } catch {
      /* 轮询失败静默 */
    }
  }, []);

  // 首次加载
  useEffect(() => {
    (async () => {
      try {
        setConfig(await api.getConfig());
      } catch (e) {
        toast.err(`读取设置失败：${e}`);
      }
      await Promise.all([refreshEnv(), refreshProfiles(), refreshProcs()]);
    })();
  }, [refreshEnv, refreshProfiles, refreshProcs]);

  // 主题跟随配置；浅色为默认
  useEffect(() => {
    document.documentElement.dataset.theme =
      config?.theme === "dark" ? "dark" : "light";
  }, [config?.theme]);

  // 运行状态轮询
  useEffect(() => {
    const t = window.setInterval(refreshProcs, 2000);
    return () => window.clearInterval(t);
  }, [refreshProcs]);

  // Ctrl+1…6 快速切页
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.ctrlKey || e.altKey || e.shiftKey) return;
      const idx = Number(e.key) - 1;
      if (Number.isInteger(idx) && idx >= 0 && idx < NAV.length) {
        e.preventDefault();
        setPage(NAV[idx].id);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // 后端事件
  useEffect(() => {
    const un: Promise<() => void>[] = [];
    un.push(onEvent<LogLine>("dsh-log", appendLog));
    un.push(
      onEvent<{ task: string; line: string }>("env-progress", (p) =>
        appendLog({ profile: p.task, stream: "system", line: p.line, ts: Date.now() })
      )
    );
    un.push(
      onEvent<{ profile: string; url: string }>("dsh-url", (p) => {
        setUrls((u) => ({ ...u, [p.profile]: p.url }));
        const cfg = cfgRef.current;
        if (!cfg?.auto_open || openedRef.current.has(p.profile)) return;
        openedRef.current.add(p.profile);
        const open = cfg.open_mode === "browser"
          ? api.openInBrowser(p.url)
          : api.openWebuiWindow(p.profile, p.url);
        open.catch((e) => toast.err(`打开 WebUI 失败：${e}`));
      })
    );
    un.push(
      onEvent<{ profile: string; code: number }>("dsh-exit", (p) => {
        openedRef.current.delete(p.profile);
        appendLog({
          profile: p.profile,
          stream: "system",
          line: `进程已退出（退出码 ${p.code}）`,
          ts: Date.now(),
        });
        refreshProcs();
      })
    );
    return () => {
      un.forEach((p) => p.then((f) => f()).catch(() => {}));
    };
  }, [appendLog, refreshProcs]);

  const saveConfig = useCallback(
    async (next: Config) => {
      await api.setConfig(next);
      setConfig(next);
    },
    []
  );

  const runningNames = useMemo(() => new Set(procs.map((p) => p.profile)), [procs]);

  const start = useCallback(
    async (name: string) => {
      try {
        openedRef.current.delete(name);
        const url = await api.dshStart(name, null);
        setUrls((u) => ({ ...u, [name]: url }));
        toast.info(`正在启动 ${name}…`);
        refreshProcs();
      } catch (e) {
        toast.err(`启动失败：${e}`);
      }
    },
    [refreshProcs]
  );

  const stop = useCallback(
    async (name: string) => {
      try {
        await api.dshStop(name);
        openedRef.current.delete(name);
        toast.ok(`已停止 ${name}`);
        refreshProcs();
      } catch (e) {
        toast.err(`停止失败：${e}`);
      }
    },
    [refreshProcs]
  );

  const openUi = useCallback(
    async (name: string) => {
      const url = urls[name] ?? procs.find((p) => p.profile === name)?.url;
      if (!url) return toast.err("还没有拿到 WebUI 地址，请稍等启动完成");
      try {
        if (config?.open_mode === "browser") await api.openInBrowser(url);
        else await api.openWebuiWindow(name, url);
      } catch (e) {
        toast.err(`打开 WebUI 失败：${e}`);
      }
    },
    [urls, procs, config]
  );

  const shared = {
    config,
    env,
    profiles,
    procs,
    logs,
    selected,
    urls,
    runningNames,
    toast,
    setPage,
    setSelected,
    setLogs,
    refreshEnv,
    refreshProfiles,
    refreshProcs,
    saveConfig,
    start,
    stop,
    openUi,
  };

  const envReady = Boolean(env?.dsh_version);

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark" aria-hidden="true">
            D
          </div>
          <div>
            <div className="brand-name">DshDesk</div>
            <div className="brand-sub">DeepSeek Harness 启动器</div>
          </div>
        </div>
        <nav>
          {NAV.map((n, i) => (
            <button
              key={n.id}
              className={`nav-item${page === n.id ? " active" : ""}`}
              onClick={() => setPage(n.id)}
              title={`${n.label}（Ctrl+${i + 1}）`}
            >
              <span className="ico" aria-hidden="true">
                <Icon name={n.icon} />
              </span>
              {n.label}
              {n.id === "home" && procs.length > 0 ? (
                <span className="badge-count">{procs.length}</span>
              ) : null}
            </button>
          ))}
        </nav>
        <div className="sidebar-foot">
          <div className="foot-row">
            <span className={`dot${envReady ? " dot-live" : ""}`} aria-hidden="true" />
            {envReady ? "环境就绪" : "环境未就绪"}
          </div>
          <div className="foot-row">
            dsh <b>{env?.dsh_version ?? "未安装"}</b>
          </div>
          <div className="foot-row">
            Node <b>{env?.node_version ?? "未安装"}</b>
          </div>
        </div>
      </aside>

      <main className="main">
        {page === "home" && <HomePage {...shared} />}
        {page === "profiles" && <ProfilesPage {...shared} />}
        {page === "plugins" && <PluginsPage {...shared} />}
        {page === "env" && <EnvPage {...shared} />}
        {page === "console" && <ConsolePage {...shared} />}
        {page === "settings" && <SettingsPage {...shared} />}
      </main>

      <ToastHost toasts={toast.toasts} />
    </div>
  );
}

export type Shared = {
  config: Config | null;
  env: EnvStatus | null;
  profiles: ProfileInfo[];
  procs: ProcStatus[];
  logs: LogLine[];
  selected: string;
  urls: Record<string, string>;
  runningNames: Set<string>;
  toast: ReturnType<typeof useToasts>;  setPage: (p: PageId) => void;
  setSelected: (n: string) => void;
  setLogs: React.Dispatch<React.SetStateAction<LogLine[]>>;
  refreshEnv: () => Promise<void>;
  refreshProfiles: () => Promise<void>;
  refreshProcs: () => Promise<void>;
  saveConfig: (c: Config) => Promise<void>;
  start: (n: string) => Promise<void>;
  stop: (n: string) => Promise<void>;
  openUi: (n: string) => Promise<void>;
};
