# Changelog

本项目的所有重要变更记录在此文件。格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/),版本号遵循 [SemVer](https://semver.org/lang/zh-CN/)。

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
