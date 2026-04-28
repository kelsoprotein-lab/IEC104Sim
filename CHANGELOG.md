# Changelog

本项目的所有重要变更记录在此文件。格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/),版本号遵循 [SemVer](https://semver.org/lang/zh-CN/)。

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
