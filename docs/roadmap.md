# Dshnext 功能补齐路线图

2026-09-06 定：对标 GitHub 同类 dsh 启动器（dsh_desktop 638★ / DSH-Launcher 220★ /
dsh-launcher 195★ / WEP-56 31★）的功能差距分析，1-9 全做，10/11 明确不做。
做完一项勾一项，附提交号。

## 第一批（低成本，纯补课）

- [x] 1. **窗口位置/尺寸记忆**（3a2f71c 后新提交） — 关窗时存几何进 config.json，开窗恢复；OS 级关闭
      （Alt+F4/任务栏）也要走保存路径（`exit_on_close_request(false)` + 自处理
      CloseRequested）。
- [x] 2. **开机自启** — `reg add/delete HKCU\...\Run /v Dshnext`，`CREATE_NO_WINDOW`
      静默执行；设置页「启动行为」加开关；`is_enabled()` 启动时回读真实状态。
- [x] 3. **主题跟随系统** — `config.theme` 增加 `"system"`；读注册表
      `AppsUseLightTheme`；设置页三档（浅色/深色/跟随系统）。
- [x] 4. **端口冲突感知** — 启动前 TCP 探测目标端口：活 Web 服务（HTTP 应答）→
      弹「直接打开现有界面」；非 web 进程 → 警告端口被占。
- [x] 5. **诊断导出** — 设置页「导出诊断」：写脱敏诊断文件（版本/环境/配置去
      API Key/日志尾部），完成后 toast + 打开所在目录。

## 第二批（新依赖或新子系统）

- [x] 6. **系统托盘 + 驻留模式** — `tray-icon` crate（静态链，不破零依赖）；
      关闭到托盘、菜单（显示/启动/停止/退出）、状态灯；驻留：跟随窗口/托盘常驻。
- [ ] 7. **启动器自更新** — GitHub Releases 版本检查 + 下载 + SHA256 失败即拒 +
      运行中 exe 改名替换；Gitee 备源。
- [ ] 8. **崩溃自愈** — 非用户主动的 Exit + 非零码 → 指数退避自动重启（上限 N 次，
      稳定后计数清零）；设置开关。
- [ ] 9. **一键离线部署** — 约定目录（data_dir/offline/）放便携 Node + dsh 包，
      环境页检测到就提供「离线安装」，复用 installs.rs 的解压/PATH 逻辑。

## 明确不做

- 10. 内嵌 WebUI/标签页（§5 决策：零依赖 + 启动器只管管理，推翻需独立讨论）
- 11. 会话管理/余额挂件/桌面宠物（侵入 dsh 数据格式，跟版本强耦合）

## 对标来源

- myYangyunfan/dsh_desktop（638★，Tauri2）：整合打包、自愈、自更新、托盘
- MarcoG-h/DSH-Launcher（220★，Electron）：多实例中枢、离线整合包、安全审计
- Ruler4396/dsh-launcher（195★，WebView2）：窗口记忆、自启、驻留、诊断导出
- WEP-56/DSH-Launcher（31★，Tauri2）：端口复用感知、标签页内嵌
