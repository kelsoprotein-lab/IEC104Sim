# Changelog

本项目的所有重要变更记录在此文件。格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/),版本号遵循 [SemVer](https://semver.org/lang/zh-CN/)。

## [1.1.2] - 2026-04-28

### 修复 / Fixed

- **macOS**: 给 `.app` bundle 加 ad-hoc 签名 (`bundle.macOS.signingIdentity: "-"`),修 v1.1.1 及之前版本下载后 macOS 弹 **"IEC104Master / IEC104Slave 已损坏,无法打开"** 的问题。原因是 Apple Silicon (以及部分新 macOS) 对完全无签名的 app 直接拒绝打开,而不是给"无法验证开发者"的可绕过提示。Ad-hoc 签名后会变成温和的"无法验证开发者",可右键 → 打开 / Add ad-hoc signing so unsigned macOS bundles no longer trigger the "is damaged, move to Trash" prompt; users still see the "unverified developer" warning but can right-click → Open.

### macOS 升级备注 / Upgrade note for macOS users

- 已经装了 v1.1.1 或之前版本并且看到"已损坏"提示的用户,**不必重装**:终端跑一行
  ```bash
  xattr -dr com.apple.quarantine "/Applications/IEC104Master.app"
  xattr -dr com.apple.quarantine "/Applications/IEC104Slave.app"
  ```
  即可正常打开。从 v1.1.2 开始下载的 dmg 不再有此问题。

## [1.1.1] - 2026-04-28

> 围绕 v1.1.0 的多 CA 能力做了完整的数据面 + 操作面收尾,并修复了 master 上一个老 bug。Patch release,任何 v1.0.9+ 的用户都可自动收到。

### Highlights / 亮点

- 🗂️ 主站数据按 CA 真隔离 / Master data is now physically per-CA — 之前 `(IOA, AsduType)` 扁平存储让多站共连接的同 IOA 互相覆盖,现在改成 `HashMap<CA, DataPointMap>`,各站独立。
- 🌳 多 CA 的连接树自动展开成 **连接 → CA 徽章 → 分类** 三层 / Tree expands to **Connection → CA badge → category** for multi-CA setups (single-CA stays flat). 每个 CA 节点的分类计数独立统计。
- 🎛️ 工具栏新增 **自定义控制 / Custom Control** 入口 — 不必先选数据点,直接弹 ControlDialog,CA 字段是当前连接已配置 CAs 的下拉选 (有需要可切到"其他"手动输入)。
- 💾 ControlDialog 记忆 CA / IOA / 命令类型 / 值字段 (持久化到 localStorage) / ControlDialog now remembers CA/IOA/command-type/value across opens & restarts — 发送成功不再自动关窗,允许用户连续给同 CA 不同 IOA 发命令。
- 🔌 修复:TLS 模式下点"断开"前端永远停在 Connected 的老 bug / Fixed the TLS-disconnect hang where the UI stayed on Connected because the receiver task never exited from a blocking read.

### Added 新增

- **主站后端**: 新类型 `MasterReceivedData = HashMap<u16, DataPointMap>` + 连接级单调 seq;`parse_and_store_asdu` 取 ASDU 头里的 CA 路由到对应桶 / New `MasterReceivedData` per-CA storage with connection-wide seq counter, points routed by ASDU CA header.
- **主站后端**: `ReceivedDataPointInfo.common_address` 字段,前端可按 CA 过滤/分组/路由控制命令 / `ReceivedDataPointInfo` carries `common_address` so the UI can filter, group, and route control commands correctly.
- **主站前端**: `App.vue` 增加 `selectedCA: number | null` 共享状态;`categoryCounts` 形状改成 connId → Map<CA, Map<category, count>> / `selectedCA` shared state; per-CA category counts.
- **主站前端**: `ConnectionTree` 检测 `common_addresses.length > 1` 时渲染 CA 徽章子节点 + 各自展开/收起;`DataTable` 按 selectedCA × selectedCategory 双重过滤 / Tree renders CA badges with independent expand/collapse; data table filters by both CA and category.
- **主站前端**: 工具栏新按钮 **自定义控制**,打开 ControlDialog,IOA 留空,CA 默认当前连接首个 / Toolbar **Custom Control** button; CA defaults to the connection's first configured one.
- **主站前端**: ControlDialog CA 字段在多 CA 连接下变成下拉 (CA 1 / CA 2 / CA 3 / 其他...);单 CA 连接保持原数字输入 / CA dropdown listing the connection's CAs in multi-CA setups, with an "Other (custom)" escape hatch.

### Changed 改进

- **主站前端**: `ValuePanel` / `DataTable` 右键控制命令直接用数据点自身的 `common_address` (该点真实来源的站),不再去 list_connections 取"第一个 CA" / Right-click control commands now use each point's own CA (its source station) instead of the connection's first CA.
- **主站前端**: ControlDialog 全部输入字段持久化到 `localStorage` (key `iec104master.controlDialog.v1`) / All ControlDialog inputs persist via localStorage.
- **主站前端**: ControlDialog 发送成功后不再自动关闭;`Toolbar` 与 `DataTable` 移除 `@sent` 关闭句柄,确认看下方 OK Xms 指示 / Dialog stays open after a successful send; confirmation comes from the existing OK indicator.
- **CI**: `gen-update-manifest.mjs::extractChangelogSection` 同时识别 `## X.Y.Z` 与 `## [X.Y.Z]` 两种风格 / Changelog section extractor recognizes both `##` styles.
- **CI**: 新 `scripts/build-release-notes.mjs` (含 vitest) 在 publish-manifest job 末尾自动把 GitHub Release body 替换成 per-OS 下载表 + 本版本 CHANGELOG section,告别"See the assets below..."占位符 / CI auto-replaces the Release body with a rich, per-platform table + the version's CHANGELOG entry.

### Fixed 修复

- **主站后端**: `MasterConnection::disconnect()` 给 `receiver_handle.await` 包了 `tokio::time::timeout(2s)`,TLS 路径下即使 read 没透出 timeout 也不会让 Tauri 命令挂死 / `disconnect()` caps the receiver join at 2 s so a stuck blocking read can't hang the command.
- **主站前端**: `Toolbar::disconnectMaster` 的 `selectedConnectionState = 'Disconnected'` 移到 `finally` 块;后端返回 NotConnected (对端已关 socket) 也不再让按钮卡在 Connected,降级为静默 / Disconnect button always reflects intent in `finally`; benign `NotConnected` is silenced.
- **主站前端**: ControlDialog `value` 字段强制 `String()` 包一层,修 `<input type="number">` 在某些路径下让 v-model 拿到 JS number 导致后端报 `invalid type: integer 123, expected a string` / Force-stringify `value` so a numeric setpoint input doesn't fail serde deserialization on the Rust side.

### Internal 内部

- 类型 `ReceivedDataPointInfo`、`ConnectionInfo` 在前后端同步更新;`pointKey` 加入 CA 防止前端缓存跨站碰撞。

## [1.1.0] - 2026-04-28

> 把 v1.0.9 → v1.0.15 这一系列搭建自动更新链路的工作正式收尾,作为面向用户的 minor release。

### Highlights / 亮点

- 🔄 应用内自动更新 / In-app auto-update via GitHub Releases — 启动 2 秒后静默检查,发现新版本弹窗提示用户更新,下载经 ed25519 签名验证后自动重启;6 小时节流,"稍后" 24 小时内不重提。
- 🔢 主站支持多公共地址 / Master supports multiple Common Addresses per connection — "新建连接" 输入逗号分隔列表 (如 `1, 2, 3`),自动 GI / 时钟同步 / 累计量召唤按列表循环。
- 🛡️ 全平台 ed25519 签名 / ed25519-signed bundles for every platform — macOS `.app.tar.gz`、Linux `.AppImage`、Windows `-setup.exe` 都带 `.sig`。
- 🛠️ Release CI 现在生成两份 manifest / CI now produces `latest-slave.json` and `latest-master.json` — 两个应用各自独立的 updater endpoint,避免混在一起。

### Added 新增

- **主站 + 从站**: `tauri-plugin-updater` / `tauri-plugin-process` / `tauri-plugin-store` 接入,新增三个 Tauri 命令 `check_for_update` / `install_update` / `snooze_update`,纯函数 `should_check` / `is_snoozed` 带 12 个单元测试 (slave + master 各 6 个) / Plugged in updater/process/store plugins; added throttle/snooze pure helpers covered by 12 unit tests.
- **主站 + 从站**: 新 Vue 组件 `UpdateDialog.vue`,展示版本号、changelog、下载进度、错误重试,中英文 i18n / New `UpdateDialog.vue` showing version, changelog, progress, retry — bilingual i18n.
- **主站**: 一个连接绑定多个 CA 的字段 `common_addresses: Vec<u16>` (后端) / `common_addresses_text: string → number[]` (前端),`ConnectionTree` 显示 `CA:1,2,3` / Multi-CA per master connection (Rust + Vue), tree shows `CA:1,2,3`.
- **CI**: 新增 `scripts/gen-update-manifest.mjs` 从 release assets 按文件名前缀拆分生成 `latest-slave.json` / `latest-master.json`,带 vitest 单测覆盖正则匹配与版本号边界 / `gen-update-manifest.mjs` produces split per-role manifests, with vitest covering regex + version boundary cases.
- **CI**: `release.yml` 新增 `publish-manifest` job,在两个 build job 完成后运行,把 manifest 上传到同一 release / `publish-manifest` job uploads both manifests after build.

### Changed 改进

- `tauri.conf.json` 新增 `bundle.createUpdaterArtifacts: true` 让 Tauri 在每个平台产出可签名的 updater bundle / Added `bundle.createUpdaterArtifacts: true` so Tauri emits signable updater bundles per OS.
- 修正 `releaseNotes.ts` 中过时的仓库 URL (旧 `IEC104Sim` 已失效) / Fixed stale repo URL in `releaseNotes.ts` (`IEC104Sim` is gone).
- 失败兜底:网络不可达、json 404、解析失败、验签失败一律 `log::warn!` + 返回 None,不打扰用户 / All failure modes (network down, JSON 404, signature mismatch) silently log and return `None` — never popup an error.

### Fixed 修复

- 自上一个正式 release v1.0.8 以来,v1.0.9 → v1.0.15 共 7 个 patch 在追 CI 链路 (sig 上传、bundle 命名、manifest 正则适配 Tauri 2 真实产物名),此版本作为正式收口 / Auto-update CI plumbing fixed across 7 iterative patches (v1.0.9–v1.0.15); this minor release rolls them up.

### Internal 内部

- spec & plan 写在 `docs/superpowers/specs/2026-04-28-tauri-auto-update-design.md` 与 `docs/superpowers/plans/2026-04-28-tauri-auto-update.md`。

### Upgrade Notes / 升级说明

- v1.0.8 及更早的用户**需要手动升级一次**到 v1.1.0 (老版本没有 updater 客户端代码)。从 v1.1.0 起,后续版本将自动收到推送。
- v1.0.9 → v1.0.15 的用户也建议手动升一次到 v1.1.0 以使用稳定的 updater 链路 (那几个 patch 里多次 CI 失败,部分版本的 release 资产可能不全)。

### Known Limitations / 已知限制

- **主站**: 多 CA 场景下右键单点控制命令仍然只发到连接的第一个 CA (数据点未携带 CA 信息) / Right-click control commands target the first CA only in multi-CA setups (data points don't carry CA info).
- macOS 应用未做公证 / macOS bundles aren't notarized — 在新版 macOS 上首次运行可能被 Gatekeeper 拦下,需要用户在系统偏好设置 → 安全性中允许。

## [1.0.15] - 2026-04-28

### 修复
- **CI**: v1.0.14 验证发现 Tauri 2 + tauri-action 在默认配置下已经把所有 `*.sig` 文件、macOS `.app.tar.gz`、Linux `.AppImage`、Windows `.exe` 都正确上传到了 release —— 我们之前自己写的 explicit upload step 完全冗余,并且基于错误的文件名假设(找 `.AppImage.tar.gz` / `.nsis.zip`,而 Tauri 2 实际产物是 `.AppImage` / `-setup.exe`)。本版本删除冗余 upload step,把 `gen-update-manifest.mjs` 的正则改成匹配 Tauri 2 真实产物名,vitest 加了"不能误匹配 .dmg/.msi/.deb/.rpm"的回归测试。

### 备注
- v1.0.14 release 里有一个 tauri-action 自动生成的 `latest.json` —— 它把 slave/master 混在一起所以不可用,但我们的 updater 端点指向的是 `latest-slave.json` / `latest-master.json`,所以无影响。`latest.json` 留在 release 里作为无害噪声。

## [1.0.14] - 2026-04-28

### 修复
- **CI**: v1.0.13 试图把 `"updater"` 放进 `bundle.targets` 数组,被 Tauri 2.10 schema 拒绝(`BundleTargetInner` 不接受这个值)。本版本改用正确的字段:`bundle.createUpdaterArtifacts: true`,Tauri 会按当前 OS 自动产出对应的 updater bundle(`.app.tar.gz` / `.AppImage.tar.gz` / `.nsis.zip`)并签名。同时去掉 `includeUpdaterJson: false`,让 tauri-action 走默认路径完成签名;find-based upload step 仍然负责把 sig + updater bundle 上传到 release。

### 新增
- **主站**: 一个连接支持多个公共地址 (CA)。在"新建连接"对话框的"公共地址 (CA)"字段输入逗号分隔的列表(例如 `1, 2, 3`),应用会在连接成功后对每个 CA 各发一次 GI;时钟同步、累计量召唤同样按列表循环。连接树显示 `CA:1,2,3`。

### 已知限制
- **主站**: 右键单点控制命令仍然只发到连接的第一个 CA(数据点未携带 CA 信息)。多 CA 且 IOA 重叠的场景下命令的目标可能不符合用户预期。

## [1.0.13] - 2026-04-28 (broken — no release artifacts)

CI build 失败:`bundle.targets` 里的 `"updater"` 被 Tauri 2.10 schema 拒绝。修复见 v1.0.14。

## [1.0.12] - 2026-04-28

### 修复
- **CI**: v1.0.11 的 upload step 用 bash glob (`target/release/bundle/.../IEC104*.tar.gz`) 在 GitHub-hosted runner 上没匹配到任何文件(具体原因待诊断,可能是 cwd / 文件清理时机问题)。本版本改用 `find target -path "*/release/bundle/.../" -name "IEC104*..."` 的方式,并新增一个 Debug 步骤打印 target 目录下所有 `.tar.gz / .zip / .sig` 文件以便排查。

## [1.0.11] - 2026-04-28

### 修复
- **CI**: v1.0.10 的修复方向正确(`includeUpdaterJson: false`)但 upload step 用了 `tauri-action` 的 `outputs.artifactPaths`,而该输出实际只列主 installer,不含 `.sig` 与 updater bundle。本版本改为按 `runner.os` 分支直接 glob `target/.../bundle/{macos,nsis,appimage}/` 目录:macOS 把 `.app.tar.gz.sig` 加上 arch 后缀防 aarch64/x64 互相覆盖;Linux/Windows 上传 `IEC104*.AppImage.tar.gz(.sig)` / `IEC104*.nsis.zip(.sig)`。
- 自此 v1.0.9 / v1.0.10 用户启动应用后将自动收到 v1.0.11 的更新提示。

## [1.0.10] - 2026-04-28

### 修复
- **CI**: 修复 release workflow 没有把 `*.sig` 文件和 updater bundles (Windows `.nsis.zip` / Linux `.AppImage.tar.gz`) 上传到 release 的问题。原因是 `tauri-action` 在多 app 同 tag 场景下生成内置 updater JSON 失败,连带跳过了 sig 上传。本版本通过设置 `includeUpdaterJson: false` 让 tauri-action 只上传 bundles + sig,manifest JSON 由独立 `publish-manifest` job 生成。
- 注:本版本 upload step 实现有缺陷,实际未正确上传 sig 和 bundle,需 v1.0.11 修复。

## [1.0.9] - 2026-04-28

### 新增
- **主站 + 从站**: 应用内自动更新。启动后 2 秒静默检查 GitHub Releases,发现新版本时弹窗显示更新说明并允许一键下载、ed25519 验签后自动重启。6 小时内不重复检查;用户点"稍后"则该版本 24 小时内不再提示。
- **CI**: release workflow 现在会同时签名安装包(`*.sig`)并生成 `latest-slave.json` / `latest-master.json` 两份 manifest 上传到 release,作为 updater 客户端的 endpoint。

### 注意
- v1.0.8 及更早版本的用户**需要手动升级一次**到 v1.0.9。从 v1.0.9 开始,后续版本将自动收到更新提示。

## [1.0.8] - 2026-04-28

### 新增
- **主站 + 从站**:UI 支持中英文运行时切换。工具栏右侧 `中 / EN` 按钮一键切换;首次启动跟随系统语言(`navigator.language` 以 `zh` 开头则中文,否则英文),用户切换后通过 `localStorage` 持久化。
- **主站**:LogPanel `详情` 列改由前端字典渲染。后端控制命令(单点 / 双点 / 步调节 / 归一化设定值 / 标度化设定值 / 浮点设定值)同时携带结构化 `detail_event { kind, payload }`,前端在切换语言时已显示的日志会立即重新渲染为新语言。
- **主站 + 从站**:LogPanel CSV 导出改为前端基于已渲染文本生成,导出文件跟随当前 UI 语言;表头与 detail 列均使用当前 locale。
- **核心库**:`LogEntry` 新增可选 `detail_event` 字段(向后兼容,序列化时 `Option::is_none` 跳过),用于前端 i18n 渲染。

### 改进
- **从站**:默认站名不再硬编码为 `站 1`。后端 `commands.rs` 创建默认 station 时传空字符串,前端 ConnectionTree 显示时回退到 `t('station.defaultName', { ca })`,实现真正的语言无关存储。

## [1.0.7] - 2026-04-27

### 新增
- **主站**:点击"连接"成功后自动发送一次总召唤(GI),无需手动再点。GI 失败仅在控制台告警,不影响连接状态。

### 改进
- **主站**:新建连接对话框的 TLS 证书路径默认填入 `./ca.pem` / `./client.pem` / `./client-key.pem`(相对路径),首次启用 TLS 即可开箱使用;localStorage 中已有空字符串的字段也会回填默认值。

## [1.0.6] - 2026-04-24

### 新增
- **主站**:新建连接对话框增加 TLS 版本策略选择(Auto / 仅 TLS 1.2 / 仅 TLS 1.3),核心层按策略约束 min/max 协议版本并附带 e2e 协商测试。
- **主站**:新建连接表单(目标地址、端口、TLS 路径、证书选项等)通过 `localStorage` 自动持久化,下次打开窗口自动回填上一次的取值。

### 改进
- **主站**:窗口标题精简为 `IEC104Master`(去除冗余后缀)。
- **主站**:移除源码中写死的本机绝对路径,避免泄露用户名与跨机失效。

### 测试
- 核心层新增 `TlsVersionPolicy` 协商用 e2e 测试,覆盖 Auto/仅 1.2/仅 1.3 三种策略的握手行为。

## [1.0.5] - 2026-04-24

### 修复
- **主站**:同一 IOA 同时配置浮点 (M_ME_NC_1) 与累计量 (M_IT_NA_1) 时,总召唤会覆盖掉前端已展示的累计量、累计量召唤会覆盖掉已展示的浮点值。数据表前端 `dataMap` 改为按 `(ioa, asdu_type)` 复合键存储,与后端一致。
- **主站**:多连接场景下树节点的类别计数与 flash 高亮被所有连接共享,一个连接执行总召唤会让另一个(已断开的)连接也显示相同数据。类别计数与变更通知改为按连接 id 分桶,实现连接隔离。

### 测试
- 新增 `crates/iec104sim-core/tests/overlapping_ioa_interrogation.rs`,覆盖"浮点 + 累计量共用同一 IOA"下 GI → CI → GI 序列中两类数据互不驱逐的行为。

## [1.0.4] - 2026-04-24

### 修复
- **主站**:从站端口关闭后,主站状态未更新为断开,且无法重连(只能删除连接后重建)。
- **主站/从站**:在输入框内按住鼠标拖选文字,若在弹窗外松开鼠标会误关弹窗。

### 改进
- **核心**:主站状态变更改用 `tokio::sync::watch` 通道统一通知,合并了原 `RwLock` + `broadcast` 的双重存储,消除 blocking 线程中的 `block_on` 调用。
- **前端**:顶栏应用名可点击打开"关于"对话框,显示当前版本与本次更新内容。

### 测试
- 新增 `crates/iec104sim-core/tests/disconnect_detection.rs`,覆盖对端关闭后的状态广播与重连路径。

## [1.0.3] - 之前

见 [v1.0.3 release notes](https://github.com/kelsoprotein-lab/IEC104Sim/releases/tag/v1.0.3)。
