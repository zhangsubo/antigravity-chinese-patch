# 贡献指南

欢迎提交翻译修正、问题报告和代码改进。汉化范围严格限定于 **Google Antigravity Desktop** 应用界面与内置 Swarm 面板，聊天内容、用户输入、代码正文和用户文件应始终保留原文。

## 开发流程

1. Fork 本仓库并创建工作分支。
2. 安装开发依赖与工具链（Node.js **20+**、Rust **1.80+**）：
   ```bash
   npm ci --ignore-scripts
   ```
3. 修改词库（`locales/`）、前端运行时（`src/`）或 Rust 宿主（`rust-src/`）。
4. 重新编译前端载荷并运行全量自动化测试（JS + Rust）：
   ```bash
   npm run build:payloads
   npm test
   cargo build --release
   ```
5. 确保 `dist/` 产物与源码同步提交。
6. 创建 Pull Request，说明修改内容、验证方法和未验证范围。

## 三层词库与 6 道编译门禁

- **主界面精确词典**：按功能模块拆分存放于 `locales/src/core/` 与 `locales/src/views/`（禁止在不同 JSON 文件中重复定义相同英文 Key）。
- **主界面正则规则**：存放于 `locales/src/rules/patterns.json`，仅用于明确带数字或短参数的动态模板。
- **Swarm 独立面板词库**：存放于 `locales/scoped/swarm.json`，与主界面词库彻底隔离，并显式声明允许匹配的选择器（`selectors` / `attributeSelectors`）与受保护容器（`protectedRoots`）。
- **编译门禁**：`tools/build-payloads.mjs` 在构建时强制执行 6 道质量门禁：
  1. **Trim & Non-Empty Gate**：禁止首尾空白 Key 或空译文；
  2. **Duplicate Key Gate**：禁止单文件内或跨分类文件出现重复英文 Key；
  3. **ASCII / Markup Gate**：禁止精确词典 Key 包含汉字，禁止译文包含 `<` / `>` 标签字符；
  4. **Regex Syntax & Double-Escape Gate**：校验正则可编译且不包含 `\\d` / `\\s` / `\\b` / `\\w` 等双重转义笔误及 `g` 标志；
  5. **Capture-Group Invariant Gate**：校验真实捕获组数量与 `replacement` 中的 `$1..$n` 引用严格匹配；
  6. **Deterministic Bundle Gate**：固定键序生成 `dist/zh-CN.bundle.json`、`dist/desktop-runtime.js`、`dist/swarm-runtime.js` 与 `dist/main-swarm-hook.js`，保证跨平台字节级确定性。

## 翻译与防误译规范

- 词条须对应真实界面的独立文本节点，并注明应用版本及所在页面。
- 优先使用完整词条精确匹配，避免宽泛正则全文替换。
- 保留模型名称、路径、版本号、数值、分支名、项目名及用户内容。
- 扩展翻译区域时，必须在 `tests/runtime.test.mjs` 或 `tests/swarm.test.mjs` 中补充不应被翻译的反例测试，尤其是聊天正文、输入框、技能、SSH 主机与项目名称。
- 对话控件须同时校验界面结构和固定文案，不得仅凭按钮文字或输入占位文案识别审批。
- AskQuestion 的问题、选项及其预览默认保留原文；权限选项仅在权限卡结构得到确认后翻译固定词条，并明确区分本次、当前对话和全局范围。包含命令或其他动态目标的选项保留原文。
- 流式正文、工具输出和代码区的变化不应触发无关界面扫描；新增控件及固定属性更新仍须验证可被识别，切换语言后须恢复原文。
- 可通过 `npm run scan:drift -- <bundle.js|app.asar>` 扫描新版客户端潜在漂移文案并辅助补充词条。

## 版本规范与发布

- 自 `3.0.0` 起采用标准三段式 **SemVer 2.0** 版本号（须同步更新 `Cargo.toml`、`package.json`、`package-lock.json` 与 `src/shared/constants.js`）。
- `tools/compatibility.json`（Schema v2）同时维护已实测版本的 `verified_versions` 精确指纹与 `anchor_rules` 弹性锚点规则。适配新的 Antigravity Desktop 版本后，须将实测版本与对应文件 SHA-256 补充到 `verified_versions` 并更新 `CHANGELOG.md`。
- 支持新版本前，须在目标平台完成只读 `doctor`、动态模式与静态模式（安装、启动、语言切换、重启、完整还原及原版签名）验证。

## 提交要求

截图和错误信息应先移除个人信息。不得提交供应商应用文件、解包源码、账号凭据、带鉴权参数的 `plugin://` URL 或真实用户数据。

安装问题与译文建议可通过 Issue 提交；如已有相同问题，请在原 Issue 中补充信息。

## 安全问题

涉及用户数据或权限的漏洞，请使用仓库的私密漏洞报告渠道联系维护者。若该渠道尚未开启，请先联系维护者确认报告方式，勿在公开 Issue 中粘贴敏感数据。
