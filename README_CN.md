# IEC 60870-5-104 Simulator

[![GitHub Release](https://img.shields.io/github/v/release/Carl-Dai/IEC60870-5-104-Simulator)](https://github.com/Carl-Dai/IEC60870-5-104-Simulator/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey)]()

[English](README.md)

基于 **Rust** + **Tauri 2** + **Vue 3** 构建的跨平台 IEC 60870-5-104 协议仿真工具，包含从站（Slave）和主站（Master）两个独立应用。

## 应用截图

### 主站 · 一条 TCP 链路上跑多个公共地址

一个 IEC 104 主站连接可以同时与多个站（Common Address）对话。在"新建连接"
对话框里把公共地址填成 `1, 2, 3`，连接树会自动展开为
**连接 → CA 徽章 → 分类** 三层结构，每个 CA 的分类计数独立统计 ——
不同站共用同一个 IOA 也不会在界面上互相覆盖。

![主站多 CA 树形展示 + 新建连接对话框](docs/screenshots/master-multi-ca-newconn.png)

### 主站 · 含 TLS 握手与多 CA 总召的通信日志

底部通信日志面板完整记录每一步 TLS 握手、U/I/S 帧、传送原因解码、原始 hex
字节。截图里主站依次发送 **GI CA=1** 和 **GI CA=2**，并接收两个站各自的
响应数据流。

![主站通信日志含 TLS 与多 CA 总召](docs/screenshots/master-multi-ca-comm-log.png)

## 下载安装

Windows、macOS 和 Linux 平台的安装包可在 [Releases](https://github.com/Carl-Dai/IEC60870-5-104-Simulator/releases) 页面下载。

## 功能特性

### 从站 (IEC104Slave)

- **IEC 104 服务端**，支持 TCP 和 TLS 连接
- **8 种数据类型**：单点、双点、步位置、位串、归一化、标度化、短浮点、累计量
- **数据点管理**：支持单个添加或批量添加（IOA 范围 + ASDU 类型选择）
- **随机变位**：按可配置间隔模拟数据变化
- **自发传送**（COT=3）：数据变化后自动向已连接主站上送
- **周期发送**：可配置间隔的周期性数据传送
- **总召唤**（GI）和**累计量召唤**响应
- **控制命令处理**：单点、双点、步调节、设定值命令
- **通信日志**：支持 hex 帧显示和 CSV 导出
- 创建服务器后自动启动

### 主站 (IEC104Master)

- **IEC 104 客户端**，支持 TCP 和 TLS 连接
- **一个连接绑定多个公共地址 (CA)**：单条 TCP 链路上同时与多个站对话；
  连接成功后自动 GI / 时钟同步 / 累计量召唤按 CA 列表逐一发送；
  接收侧按 CA 分桶存储，不同站的同 IOA 不互相覆盖
- **多 CA 三层连接树**：连接 → CA 徽章 → 分类，每个 CA 的分类计数独立；
  单 CA 连接保持原扁平树
- **实时数据显示**：增量轮询 + 虚拟滚动
- **分类树**：实时显示各类别点数（单点、双点、步位置、位串、归一化、标度化、浮点、累计量）
- **工具栏 "自定义控制" 按钮**：弹出独立控制对话框，CA 字段下拉选当前
  连接已配置的 CAs，IOA 任意输；发送成功后窗口保留以便连续发命令；
  CA / IOA / 命令类型 / 值字段持久化到 localStorage，跨打开和重启都记得
- **控制命令**：直接执行和选择-执行（SbO）；右键控制命令直接路由到
  数据点自身的 CA（多 CA 场景下不会发错站）
- **值面板**：显示选中数据点详情
- **总召唤**、**累计量召唤**、**时钟同步**命令
- **通信日志**：含 TLS 握手事件、U/I/S 帧解码、COT 中文化、原始 hex
  字节并排显示；支持 CSV 导出
- **应用内自动更新**：从 GitHub Releases 推送（ed25519 签名验证、6 小时
  检查节流、"稍后" 24 小时不重提）

## 项目结构

```
IEC104Sim/
├── crates/
│   ├── iec104sim-core/     # IEC 104 协议核心库
│   ├── iec104sim-app/      # 从站 Tauri 应用
│   └── iec104master-app/   # 主站 Tauri 应用
├── frontend/               # 从站 Vue 3 前端
└── master-frontend/        # 主站 Vue 3 前端
```

## 环境要求

- [Rust](https://rustup.rs/) (1.77+)
- [Node.js](https://nodejs.org/) (18+)
- [Tauri CLI](https://tauri.app/) (`cargo install tauri-cli`)

## 快速开始

### 安装依赖

```bash
cd frontend && npm install
cd ../master-frontend && npm install
```

### 启动从站

```bash
cd crates/iec104sim-app
cargo tauri dev
```

### 启动主站

```bash
cd crates/iec104master-app
cargo tauri dev
```

### 使用流程

1. **从站**：点击"新建服务器" → 自动在 2404 端口启动，带默认数据点
2. **主站**：点击"新建连接" → 输入 `127.0.0.1:2404` → 连接 → 发送总召唤
3. 主站 IOA 表格显示所有接收到的数据点
4. **从站**：点击"随机变化"模拟数据变位 → 主站实时收到自发上送数据

## IEC 104 协议支持

| 功能 | 支持类型 |
|------|---------|
| 监视方向（从站→主站） | M_SP_NA/TB, M_DP_NA/TB, M_ST_NA/TB, M_BO_NA/TB, M_ME_NA/TD, M_ME_NB/TE, M_ME_NC/TF, M_IT_NA/TB |
| 控制方向（主站→从站） | C_SC_NA, C_DC_NA, C_RC_NA, C_SE_NA/NB/NC |
| 系统命令 | C_IC_NA（总召唤）、C_CI_NA（累计量召唤）、C_CS_NA（时钟同步） |
| 传输原因 | 突发(3)、激活(6)、激活确认(7)、激活终止(10)、总召唤(20)、累计量召唤(37) |
| 传输层 | TCP、TLS（支持双向 TLS） |

## 技术栈

- **后端**：Rust、Tokio（异步运行时）、native-tls
- **前端**：Vue 3、TypeScript、Vite
- **桌面端**：Tauri 2

## 更新日志

最新变更请参见 [CHANGELOG.md](CHANGELOG.md) 或 [Releases 页面](https://github.com/Carl-Dai/IEC60870-5-104-Simulator/releases)。

### 自动更新

从 v1.0.9 起，两个应用在启动时自动检测 GitHub Releases，发现新版本会弹窗提示安装。
v1.0.8 及更早版本的用户需要手动升级一次到 v1.0.9，之后将自动收到后续更新。

### macOS 安装提示

应用未做 Apple 公证（Notarization）。从 v1.1.2 起 dmg 内的 .app 带 ad-hoc 签名，
首次打开时 macOS 会提示"无法验证开发者"，**右键 → 打开** 即可绕过。

如果你下载的是 v1.1.1 或更早的 dmg，看到 **"已损坏，无法打开"** 提示，是因为
旧版完全没签名，被新 macOS 直接判定为损坏。终端跑一行解决：

```bash
xattr -dr com.apple.quarantine "/Applications/IEC104Master.app"
xattr -dr com.apple.quarantine "/Applications/IEC104Slave.app"
```

或直接升级到 v1.1.2 及以后的版本（应用内"检查更新"也会推过来）。

## 许可证

MIT
