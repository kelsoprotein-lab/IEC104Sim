# IEC 104 模拟器

[English](README.md)

基于 **Rust** + **Tauri** + **Vue 3** 构建的跨平台 IEC 60870-5-104 协议模拟器，包含从站（Slave）和主站（Master）两个独立应用。

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
- **实时数据显示**：增量轮询 + 虚拟滚动
- **分类树**：实时显示各类别点数（单点、双点、步位置、位串、归一化、标度化、浮点、累计量）
- **控制命令**：直接执行和选择-执行（SbO）
- **右键菜单**：快速控制操作
- **值面板**：显示选中数据点详情
- **总召唤**、**累计量召唤**、**时钟同步**命令
- **通信日志**：帧解析显示

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

## 许可证

MIT
