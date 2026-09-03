import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface Config {
  api_key: string;
  port: number;
  node_mirror: string;
  npm_registry: string;
  plugin_catalog_url: string;
  open_mode: string; // "window" | "browser"
  auto_open: boolean;
  theme: string; // "light" | "dark"
}

export interface EnvStatus {
  node_version: string | null;
  node_path: string | null;
  node_managed: boolean;
  pnpm_version: string | null;
  dsh_version: string | null;
  dsh_path: string | null;
  data_dir: string;
  home_dir: string;
}

export interface ProfileInfo {
  name: string;
  bundles: string[];
  dependencies: Record<string, string>;
  path: string;
}

export interface PluginInfo {
  name: string;
  version: string;
}

export interface MarketItem {
  name: string;
  source: string;
  description: string;
  stars: number;
  version: string;
  origin: string;
}

export interface ProcStatus {
  profile: string;
  port: number;
  url: string;
  pid: number;
  uptime_secs: i64_alias;
}
type i64_alias = number;

export interface DshVersions {
  latest: string | null;
  versions: string[];
}

export interface NodeRelease {
  version: string;
  lts: string | null;
}

export interface LogLine {
  profile: string;
  stream: string;
  line: string;
  ts: number;
}

export interface EnvProgress {
  task: string;
  line: string;
}

export const api = {
  getConfig: () => invoke<Config>("get_config"),
  setConfig: (config: Config) => invoke<void>("set_config", { config }),
  envStatus: () => invoke<EnvStatus>("env_status"),
  dshVersions: (includeRc: boolean) => invoke<DshVersions>("dsh_versions", { includeRc }),
  nodeVersions: () => invoke<NodeRelease[]>("node_versions"),
  installNode: (version: string) => invoke<void>("install_node", { version }),
  installDsh: (version: string) => invoke<void>("install_dsh", { version }),
  installPnpm: () => invoke<void>("install_pnpm"),
  uninstallNode: () => invoke<void>("uninstall_node"),
  uninstallDsh: () => invoke<void>("uninstall_dsh"),
  profileList: () => invoke<ProfileInfo[]>("profile_list"),
  profileCreate: (name: string) => invoke<void>("profile_create", { name }),
  profileCopy: (src: string, dst: string) => invoke<void>("profile_copy", { src, dst }),
  profileRename: (src: string, dst: string) => invoke<void>("profile_rename", { src, dst }),
  profileDelete: (name: string) => invoke<void>("profile_delete", { name }),
  dshStart: (profile: string, port: number | null) =>
    invoke<string>("dsh_start", { profile, port: port ?? null }),
  dshStop: (profile: string) => invoke<void>("dsh_stop", { profile }),
  dshStatus: () => invoke<ProcStatus[]>("dsh_status"),
  pluginList: (profile: string) => invoke<PluginInfo[]>("plugin_list", { profile }),
  pluginAdd: (profile: string, source: string) =>
    invoke<void>("plugin_add", { profile, source }),
  pluginRemove: (profile: string, name: string) =>
    invoke<void>("plugin_remove", { profile, name }),
  marketItems: () => invoke<MarketItem[]>("market_items"),
  openProfileDir: (profile: string) => invoke<void>("open_profile_dir", { profile }),
  openDataDir: () => invoke<void>("open_data_dir"),
  openInBrowser: (url: string) => invoke<void>("open_in_browser", { url }),
  openWebuiWindow: (profile: string, url: string) =>
    invoke<void>("open_webui_window", { profile, url }),
};

export function onEvent<T>(name: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(name, (e) => handler(e.payload));
}
