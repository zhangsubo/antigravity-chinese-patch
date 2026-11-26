# 更新日志

## [3.0.5] - 2026-11-26

- 新增 Antigravity Desktop **2.35.2**（Build `2.35.2-4585983447007232`）三平台精确指纹（跨平台 `resources/app.asar` SHA-256 `e057115571cf5085406d20f2427a5afac272b712ec84332c7157da0113d4b2d0`；`dist/preload.js`、`dist/main.js`、`dist/tray.js`、`dist/cablev2.js`、`dist/menu.js`、`dist/updater.js` 共 24 条规则）。
- 补充 Desktop 2.35.2 模型选择器新提示词条（`Applies to /btw and messages sent after this turn`、`Applies to messages sent after this turn`，同时保留旧版词条兼容性；主界面精确词条增至 **2,337 条**，动态规则 **64 条**；Swarm 面板 **156 条**精确词条 + **11 条**规则）。
- Linux x64 已在 Instabox `box-856891`（Debian 13 x86_64 + XFCE `DISPLAY=:1`）完成 `2.35.2` 静态补丁全生命周期（`doctor` / 非特权拦截 / `install` / 实机切换 / 运行期还原拦截 / 重启保持中文 / `restore` 无损还原）与 `agy-zh run` 动态守护模式（含主界面、设置中心、Swarm 子帧中英同步）真实桌面实测；Windows 11 x64 已完成官方 `2.35.2` 静态补丁全生命周期、CDP 动态守护（含主界面、设置中心与 Swarm 子帧同步切换）、桌面 `.lnk` 快捷方式及客户端退出后守护自退零弹窗（[#1](https://github.com/zhangsubo/antigravity-chinese-patch/issues/1)）真机实测验收，`2.35.2` 的 `native_ui_tested_platforms` 启用 `["linux", "win32"]`；macOS 已核验 2.35.2 官方包指纹与离线 `doctor`，真机桌面待手工测试验收。
- 修复 Windows 上通过管道捕获 `agy-zh run` 标准输出时子进程继承标准句柄导致调用方阻塞的问题（在派生后台守护及客户端进程前清除标准句柄继承标志 `HANDLE_FLAG_INHERIT`），修复非管理员/未开启开发者模式 Windows 环境下 `cargo test` 创建测试目录链接的权限报错（回退 `mklink /J` 目录联接并规范化路径分隔符），加固 Windows 下 `leave_app_dir_if_inside` 路径大小写不敏感匹配，并补充 Windows / Linux 快捷方式沙盒单测（29 项 Rust 单测 0 warning 全通过）。

## [3.0.4] - 2026-11-25

- 新增 Antigravity Desktop **2.35.0**（Build `2.35.0-6394266482769920`）三平台精确指纹（`dist/preload.js`、`dist/main.js`、`dist/tray.js`、`dist/cablev2.js`、`dist/menu.js`、`dist/updater.js` 共 24 条规则）。
- 适配 Desktop 2.35.0 新增界面与边界保护（主界面精确词条增至 **2,335 条**，动态规则增至 **64 条**；Swarm 面板增至 **156 条**精确词条 + **11 条**规则）：
  - 自定义窗口控件与左上角应用图标菜单（`custom-window-controls`、`app-logo-menu`）、侧边栏收件箱模式与自定义菜单（`customize-sidebar-menu`、`Inbox Mode`、`Auto-Resolve`、`Working Section`、`Resolve conversation`、`Move Conversation`）、新建个人/团队智能体菜单（`new-agent-menu`）及悬浮卡片状态标签（保护标题、描述与邮箱）；
  - 新版内联权限/问答卡片（`inline-permission-card` 的 `Yes, allow for this task` / `Yes, always allow in this project` 与 `permission-trigger-command`、保护命令文本；`inline-card` 的已处理标签与自由回答输入框）、插件权限设置与撤销弹窗（`plugin-permissions-*`、`plugin-grant-card`）、语音听写语言多选设置（`voice-settings-dictation-languages`，保护语言原名）以及消息加载与排队发送状态栏；
  - 补充 2.35.0 设置项（如 `Prevent sleep`、`Keep in menu bar`、`Notification settings` 等 sentence-case 标签）与 Swarm「加载更早消息」动态提示（`swarm-load-earlier`）。
- 修复 Windows 上模式一（`agy-zh run`）退出 Antigravity 后无限弹出 PowerShell 控制台窗口的问题（[#1](https://github.com/zhangsubo/antigravity-chinese-patch/issues/1)）：`run_cmd` 与守护进程启动统一切换 `CREATE_NO_WINDOW`，存活判定收紧为仅匹配 `Antigravity.exe` 主进程（`language_server.exe` 等 `resources\` 残留不再误判客户端存活），Windows 进程探测改为原生 Toolhelp32 枚举（不再 shell 出 PowerShell）并加 5 秒冷却，`agy-zh stop` 在 Windows 上 2.5 秒后强制结束守护进程、轮询睡眠支持 100ms 级中断响应。真机验收待完成。
- Linux x64 已在 Instabox `box-856891`（Debian 13 x86_64 + XFCE `DISPLAY=:1`）完成 `2.35.0` 静态补丁全生命周期（`doctor` / 非特权拦截 / `install` / 实机切换 / 运行期还原拦截 / 重启保持中文 / `restore` 无损还原）与 `agy-zh run` 动态守护模式（含主界面、设置中心、Swarm 子帧中英同步）真实桌面实测；macOS 与 Windows 已核验 2.35.0 官方包指纹，真机桌面待手工测试验收。

## [3.0.3] - 2026-11-22

- 新增 Antigravity Desktop **2.33.1** 和 **2.34.0** 精确指纹。两个版本均已在 macOS arm64 和 Linux x64 验证静态安装、还原及动态汉化。Windows x64 已核验发行包指纹，桌面实测待完成。
- 适配 2.34.0 的侧边栏自定义、对话筛选、插件市场、主控模型选择器、内联权限卡片及终端和版本控制弹窗。主界面词库包含 2,164 条精确词条和 58 条规则。
- Swarm 看板新增「蜂巢」「提及」「导出 GIF」及预览弹窗等译文，共 153 条专用词条和 7 条规则。
- 修复 macOS 2.34.0 独立 Passkey 弹窗中的连接提示。静态补丁与动态模式均随主窗口语言开关切换。

## [3.0.2] - 2026-11-20

- 收录 macOS、Linux、Windows 三平台 Antigravity Desktop `2.33.0`（Build `2.33.0-4731201358004224`）精确指纹与 `dist/preload.js` / `dist/main.js` / `dist/tray.js` / `dist/menu.js` / `dist/updater.js` 共 23 条原生界面替换规则。
- 在 `tools/compatibility.json` 与 `rust-src/compatibility.rs` 引入分平台 `native_ui_tested_platforms` 字段：`2.33.0` 的 `darwin` 与 `linux` 为 `true`，`win32` 保持 `false`（待实机桌面验收）。
- 收紧 `rust-src/platform.rs` 中 `is_app_running` 的进程匹配为各平台可执行文件路径前缀匹配，避免调用方 CLI 参数或外壳脚本提及应用路径时触发运行状态误判。
- 适配 Desktop `2.33.0` 新增界面与文案（主界面精确词条增至 **1,977 条**，动态正则增至 **54 条**）：
  - 设置中心「浏览器」新增浏览数据清理与按站点管理 Cookie 弹窗（`browsing-data-row`、`browser-site-data-dialog`、`browser-clear-data-confirm` 及站点/Cookie 数量正则，严格保护域名与协议头）；
  - 设置中心「快捷键」新增 VS Code 键位方案与跳转快捷键说明（`Keybinding Scheme` / `VS Code Default` / `VS Code Keyboard Shortcuts`）；
  - 首次进入工作区条款确认弹窗（`post-onboarding-terms-body`）、入门指引弹窗（`post-onboarding-welcome-modal`）与首页多模板问候语（`home-greeting`，保留用户显示名称）；
  - 自定义配置「插件套装（Bundles）」页面与来源风险审查弹窗（`bundles-page`、`bundle-detail`、`plugin-source-caution-modal`）、外部链接确认弹窗（`external-link-confirm-modal`）、Jujutsu 工作树选择器（`jj-worktree-selector-trigger`）、追问插入主对话、子智能体状态提示、访问受限屏及浏览器/阅读工具栏按钮。
- Linux x64 已在 Instabox `box-856891`（Debian 13 x86_64 + XFCE `DISPLAY=:1`）完成 `2.33.0` 静态补丁（`doctor` → 非特权拦截 → `install` → 实机中英文切换 → 运行期 `restore` 拦截 → 重启保留中文 → `restore` 逐字节还原）与 `agy-zh run` 动态守护模式实机验证；macOS arm64（macOS 27.2.1）已完成 `2.33.0` 静态补丁生命周期（`doctor` / 确认参数拦截 / `install` / `codesign` 校验 / 实机中英文切换 / 运行期拦截 / 重启恢复 / `restore` 及 `spctl` 公证恢复）与 `agy-zh run` / `doctor` / `stop` 动态守护模式全闭环真机验证；Windows 已完成官方发行包 `app.asar` 指纹与规则校验，实机桌面待手工验收。

## [3.0.1] - 2026-11-18

- 收录 macOS、Linux x64 的 Antigravity Desktop 2.32.2 精确指纹，沿用 23 条原生界面替换规则。Linux x64 已验证动态汉化、静态安装、重启与还原。Windows 2.32.2 继续使用结构锚点。
- `doctor` 区分守护进程存活与客户端 CDP 端口可达。更新重启造成持续断连时，`doctor` 和 `run` 提供恢复步骤。守护进程会在连接持续中断后释放单实例锁。
- 补齐 Desktop 2.32.2 首次登录后的个性化问卷文案与职业选项。问候语保留账号显示名称，主界面精确词条增至 1,878 条。

## [3.0.0] - 2026-11-17

### 架构重构与新增特性

- **双模汉化架构（零改包动态守护 + 静态 ASAR 安全补丁）**：
  - **模式一（推荐）：零改包 CDP 动态守护（`agy-zh run` / `agy-zh shortcut` / `agy-zh stop`）**：完全不修改 `Antigravity.app` 原包，保留 Google 官方签名与 Apple 公证，客户端自动更新不掉汉化；通过 `127.0.0.1` 回环调试端口（绝不开启 `--remote-allow-origins=*`）自动发现并幂等注入主界面与内置 Swarm 子帧，支持一键生成 macOS `Antigravity 中文.app`、Windows 桌面 `.lnk` 及 Linux `.desktop` 快捷方式；
  - **模式二：静态 ASAR 安全补丁（`agy-zh install` / `agy-zh restore`）**：保留内存级 ASAR 分块哈希重写与签名校验；macOS 升级为 APFS `copyfile` 写时复制秒级克隆；全面覆盖主界面、Swarm 面板（Electron `did-frame-finish-load` 钩子）、原生托盘、退出确认框、更新提示与会话右键菜单；
  - **分层弹性版本兼容（Schema v2）**：`tools/compatibility.json` 在保留 `2.22.0`–`2.32.1` 精确指纹校验的同时引入 `anchor_rules` 结构锚点弹性注入，对未收录新版 Antigravity Desktop 自动适配（并严格拒绝一切 Antigravity IDE / VS Code 包）；
  - **100% 兼容存量 v2 备份**：统一命名空间为 `__antigravity_chinese_patch__`，兼容识别旧版 `/* TeacherXuAI Antigravity Chinese UI` 标记与 `schema: 1` 状态记录，老用户可直接 `doctor` 或 `restore` 无损还原。
- **Rust 单文件跨平台宿主 `agy-zh`**：
  - 移除运行期 `Python + Node.js` 双栈依赖，构建零外部运行时依赖的纯 Rust 单文件二进制（内嵌确定性前端载荷，采用无 TLS 本地 `tungstenite` WebSocket 与原生系统接口），支持子命令与无参交互式中文菜单；
  - CI/CD 全面升级为 Ubuntu / macOS / Windows 测试矩阵与 6 架构单文件 Release 自动分发（macOS arm64/x64、Linux x64/arm64、Windows x64/arm64）。
- **内置 Swarm 面板作用域隔离汉化**：
  - 新增 `locales/scoped/swarm.json`（**142 条**精确词条 + **7 条**正则）与 `src/swarm/` 独立运行时，仅匹配 `plugin://swarm--swarm_board-<id>/` 帧及 `[data-testid="swarm-tabs"]` / `[data-testid="swarm-empty-state"]` 结构标记，严格保护 `#ov-frame`、`#raw`、`#swarm-title`、`.msg-body`、成员名、`@all`、`/swarm` 命令及讲述者自定义文案，并通过 `postMessage` 与主窗口实时同步中英文切换。
- **三层模块化词库、6 道编译门禁与漂移扫描**：
  - 将原扁平词典拆分为 `locales/src/core/`、`locales/src/views/`、`locales/src/rules/patterns.json`（主界面 **1,873 条**精确词条 + **47 条**规则）与 `locales/scoped/swarm.json`，将原单文件运行时拆分为 `src/shared/`、`src/runtime/` 与 `src/swarm/`；
  - 新增 `tools/build-payloads.mjs`（Trim/Non-Empty、Duplicate Key、ASCII/Markup、Regex Syntax/Double-Escape、Capture-Group Invariant、Deterministic Bundle 共 6 道门禁）与 `tools/drift-detector.mjs`（官方 JS Bundle / ASAR 漂移与假阳性扫描）。
- **新一代交互与自定义词表**：
  - 升级为可拖拽记忆位置的新版悬浮控制球，支持可视区边界吸附与多窗口 `localStorage` 状态同步；
  - 右键或双击悬浮球打开配置菜单，新增**「选项名保留英文」**预设开关（支持保留 `This Conversation`、`Request Review`、`Default`、`Auto`、`Turbo`、`High`、`Medium`、`Low` 英文选项名）；
  - 新增可选 `custom-translations.txt` 用户自定义词表（支持译文覆写与置空强制保留英文，带软链、大小及权限校验）。

## [2.32.1.3] - 2026-11-16

### 新增与改进

- 新增 **Windows 版 Antigravity Desktop 2.32.1** 安装、诊断与完整还原支持：
  - `tools/patcher.py` 支持 Windows 常见安装路径自动探测（`%LOCALAPPDATA%\Programs\Antigravity`、`%LOCALAPPDATA%\Antigravity`、`%ProgramFiles%\Antigravity` 等及 `PATH` 可执行文件定位）与 `--app` 自定义路径；
  - 支持 Windows 完整目录原子备份（`<app>.zh-cn-original`）与状态记录（`<app>.zh-cn-state.json`，含 `platform: "win32"`、`original_asar`、`patched_asar` 与 `original_executable` SHA-256 校验）、`--replace-stale-backup` 升级备份替换、中断恢复、基于 PowerShell `Get-CimInstance Win32_Process` 的运行中进程（`Antigravity.exe` / `resources\`）拦截、路径大小写不敏感匹配、目录联接（Junction）与符号链接越界防护、UTF-8 编码兼容与重命名前自动脱离应用目录句柄；
  - 新增 `.gitattributes`（`* text=auto eol=lf`）统一跨平台换行符，并更新 `package.json` 跨平台 `npm test` / `npm run build` / `npm run doctor` 脚本；
  - 新增 4 项 `WindowsInstallerTests` 单元测试与跨平台联接测试辅助（累计 20 项 Python 测试 + 26 项 JS DOM 测试），并在 `.github/workflows/test.yml` 加入 `windows-latest` CI 矩阵。

## [2.32.1.2] - 2026-11-15

### 新增与改进

- 新增 **Linux 版 Antigravity Desktop 2.32.1** 安装、诊断与完整还原支持：
  - `tools/patcher.py` 支持 Linux 应用目录自动探测（`/opt/Antigravity`、`/usr/share/antigravity`、`~/.local/share/antigravity`、`~/Antigravity-x64` 及 `PATH` 可执行文件定位）与 `--app` 自定义路径；
  - 支持 Linux 完整目录原子备份（`<app>.zh-cn-original`）与状态记录（`<app>.zh-cn-state.json`，含 `original_asar`、`patched_asar` 与 `original_executable` SHA-256 校验）、`--replace-stale-backup` 升级备份替换、中断恢复、运行中进程（`antigravity` / `resources/`）拦截、防符号链接越界校验与权限不足清晰报错，同时完整保留 macOS 签名与公证校验链路。
- 适配 Linux 专属及跨平台文件管理界面文案，词库由 1,848 条扩充至 **1,873 条**简体中文词条：
  - 新增 Linux 顶部自定义菜单栏（`[data-testid="title-menu-bar"]`：`Workspace` / `View` / `Window`、`Version X.Y.Z` 动态版本号、`Check for Updates`、`Command Palette`、`Minimize`、`Maximize`、`Close`）与首次启动登录引导页（`[data-testid="onboarding-brand-shell"]`：`欢迎使用 Antigravity！`、`通过 Google 继续`、`使用企业账号`、`登录遇到问题？` 等）翻译，并严格保护首个应用标题项（`Antigravity` 及 SSH 主机 / WSL 发行版后缀）；
  - 新增项目与文件操作菜单中的跨平台文件管理器定位文案（`Show in File Manager` / `Reveal in Finder` / `Show in File Explorer` 及失败提示）。
- 新增 Linux 安装器与运行时单元测试（26 项 JS DOM 测试 + 16 项 Python 测试），并在 OVH Linux x86_64 与 Instabox `box-856891`（Debian 13 x86_64 + XFCE 桌面环境 `DISPLAY=:1`）上对官方 `Antigravity 2.32.1`（`2.32.1-6001966350991360`）完成 `doctor（默认路径 /opt/Antigravity 自动探测） -> 非特权权限拦截 -> install -> XFCE 真机桌面启动与中英文切换截图/CDP 验证 -> restore` 全闭环验证。

## [2.32.1.1] - 2026-11-14

### 新增与改进

- 新增 **Antigravity Desktop 2.32.1** 版本适配与 `dist/preload.js`、`dist/main.js`、`dist/tray.js` SHA-256 指纹校验，并在 Apple Silicon Mac 上完成安装、签名、`--replace-stale-backup` 升级、Electron 44.9.1 实机运行、中英文实时切换、重启及完整还原验证。
- 词库由 1,672 条扩充至 **1,848 条**简体中文词条。
- 适配 2.32.1 新版界面：创建项目向导弹窗全流程（`project-folder-selection-step` 本地/云端远程切换、文件夹/资源添加、云端资源挂载说明弹窗 `cloud-resource-disclosure-modal`、设置项目名称与同名/同目录重复项目提示）、分支与工作树选择器（`Work in a New Worktree` / 历史工作树 / 分支搜索与 `· 新工作树` 后缀）、模型选择器胶囊（`新` / `快速` / `提示`）与用量配额子菜单（`本周额度` / `今日额度` / `5 小时额度` / `剩余 %`）、SSH 远程连接弹窗（`ssh-connect-dialog`）、SSH 机器切换器（`ssh-machine-switcher`）与主机状态标签（`ssh-host-label`）、macOS 辅助功能与屏幕录制权限引导卡（`computer-permissions-card`）、对话级权限预设选择器与终端沙盒动态提示（`permissions-selector-trigger` / `permissions-menu` / `permissions-sandbox-hint`）、外部资源选择器（`provider-resource-filter` / `google-drive-connect-view`）、环境初始化脚本验证控件（`setup-script-*`）、侧边栏扩展分组与显示筛选（`plugins-section-header` / `plugins-visibility-trigger`）、多智能体实时画面面板（`[data-agy-agents-pane]`）、内置浏览器地址栏评论草稿胶囊（`webview-url-bar-draft-capsule`）与评论草稿编辑框/卡片（`comment-box` / `comment-card` / 下载进度提示）、侧边面板新建/固定标签页控件、文件树搜索失败提示、模型列表刷新状态及初始引导授权码切换入口。
- 边界保护加固：严格保护创建项目与项目设置中已选本地文件夹/上传文件名称、重复项目提示中的现有项目名、分支选择器中的仓库名与分支名（`branch-repo-label`、`branch-option`、`branch-pill-label`、`branch-pill-trigger`）、内置浏览器评论正文（`comment-card-text`）、SSH 主机名与自定义配置文件名（`ssh-host-project-item`、`ssh-connect-host-option` 首列、`ssh-connect-error` 主机名、`ssh-config-file-name` 自定义文件名）、WSL 发行版名称（`ssh-machine-switcher-wsl-distro`）、GitHub 仓库条目（`project-selector-github-repo-item`、`github-repo-option-*`）、侧边栏扩展项名称（`plugin-sidebar-item`、`plugin-sidebar-mcp-app-item`、`plugins-visibility-option`），以及多智能体实时面板中的窗口标题、智能体名称与光标标签（`[data-agy-chip]`、`[data-agy-pill]`、`[data-agy-window-label]` 等）。

## [2.31.2.1] - 2026-11-12

### 新增与改进

- 新增 **Antigravity Desktop 2.31.2** 版本适配与 `dist/preload.js`、`dist/main.js`、`dist/tray.js` SHA-256 指纹校验。
- 词库由 1,397 条扩充至 **1,672 条**简体中文词条。
- 适配 2.31.2 新版界面：设置中心「浏览器」独立页签与「界面布局 / 对话布局」、项目/运行环境设置导航、协调器（Coordinators）侧边栏与管理弹窗、无折叠按钮权限审批卡（可编辑权限目标、浏览器下载详情与沙盒外运行安全提示）、选中引用与追问（`/btw` / Side Question）控件、内置浏览器下载抽屉/证书拦截页/安全信息弹层、对话遥测状态按钮、文件树筛选与 VCS 状态点、CSV 查看器外围工具栏，以及原生右键菜单 `(Default)` 动态后缀。
- 边界保护与观测加固：保护对话气泡视图（`[data-bubble-side="agent"]`）、进度响应流、追问用户消息、语音转写摘要、下载目录路径与下载文件名按钮（`webview-download-open`）、GCP 项目列表、设备选择器选项、LSP 悬停卡片及 CSV 单元格；修复动态插入受控 `input` / `textarea` 时被 `SKIP` 自匹配拦截导致未触发首轮属性翻译的问题。
- 安装器增强：`doctor` 新增 `stale_backup_version` 检测，`install` 新增显式 `--replace-stale-backup` 参数，支持应用自动更新后安全替换旧版原版备份（失败自动事务回滚）。

## [0.2.0] - 2026-11-11

### 新增与扩展

- 新增 **Antigravity Desktop 2.23.0 / 2.29.1** 版本适配与多文件 SHA-256 校验支持。
- 词库由 92 条扩充至 **1,397 条**简体中文词条。
- 覆盖设置中心全部页面（通用/语音模式、应用、外观、快捷键、通知、模型与用量、对话）、主界面「自动化（Automations）」及「自定义配置（Customizations）」插件市场/已安装页。
- 覆盖对话流权限审批卡片（严格结构校验）、思考折叠头、计划审阅栏，以及右侧面板外围工具栏（终端、Diff、内置浏览器、文件树）。
- 支持 macOS 原生会话右键菜单（通过 `preload.js` 拦截转发，支持语言开关实时切换并保护用户自定义分组名）、原生退出确认弹窗及状态栏托盘菜单汉化。
- 修复多文本子节点拼接（`translateMultiTextElement`）与 JSX 行内元素两侧纯空格分隔节点（`inlineConcat`）的识别边界。
- 新增 21 项 JavaScript DOM 回归测试与 11 项 Python 安装器测试，并配置 GitHub Actions Release 自动打包发布工作流。

## [0.1.1] - 2026-10-11

### 文档修订

- 统一项目名称、章节标题及发布说明的文档格式。
- 整理兼容性、安全说明、下载、安装、验证、中英文切换及原版还原步骤。
- 补充项目结构、问题反馈要求和更新日志。
- 更新分发包中的文档与版本标识。

词库、翻译逻辑、安装和还原流程与 v0.1.0 保持一致。已安装 v0.1.0 的用户无需为本次文档修订重新安装补丁。

## [0.1.0] - 2026-10-11

### 首次发布

- 为 macOS 上的 Antigravity Desktop 2.22.0 提供 92 条简体中文译文。
- 支持主界面中英文切换，重启后默认开启中文。
- 提供版本与文件校验、完整原版备份及还原工具。
- 收录 6 张已遮挡账号的实机截图与 3 张中英文对比图。
- 完成 19 项自动化测试，以及 Apple Silicon Mac 上的安装、切换、重启和完整还原验证。

[2.32.1.3]: https://github.com/zhangsubo/antigravity-zh-cn/releases/tag/v2.32.1.3
[2.32.1.2]: https://github.com/zhangsubo/antigravity-zh-cn/releases/tag/v2.32.1.2
[2.32.1.1]: https://github.com/zhangsubo/antigravity-zh-cn/releases/tag/v2.32.1.1
[2.31.2.1]: https://github.com/zhangsubo/antigravity-zh-cn/releases/tag/v2.31.2.1
[0.2.0]: https://github.com/zhangsubo/antigravity-zh-cn/releases/tag/v0.2.0
[0.1.1]: https://github.com/zhangsubo/antigravity-zh-cn/releases/tag/v0.1.1
[0.1.0]: https://github.com/zhangsubo/antigravity-zh-cn/releases/tag/v0.1.0
