# 主站 TLS 版本选择 — 设计方案

- 日期: 2026-04-24
- 目标版本: v1.0.6
- 范围: 仅主站 (`iec104master-app` + `iec104sim-core::master`)

## 1. 背景 & 问题

当前 `MasterConnection::create_tls_stream` (`crates/iec104sim-core/src/master.rs:418`) 将协议版本硬编码为 `min_protocol_version(Some(Tlsv12))`,未设 `max_protocol_version`。结果:

- 无法强制使用 TLS 1.2(某些旧 IEC 62351 装置不支持 1.3)。
- 无法强制使用 TLS 1.3(某些新设备要求禁用 1.2)。
- 协议对接/排障时无法用同一客户端切换版本验证。

用户需求: 在"新建连接"弹窗里支持选择 TLS 版本,三选一。

## 2. 用户可选值

| 选项 | 标签 | 语义 |
|---|---|---|
| `Auto` | 自动 | `min=TLS 1.2`,不设 `max` — 由 TLS 栈向上协商。默认值。 |
| `Tls12Only` | 仅 TLS 1.2 | `min=max=TLS 1.2`。 |
| `Tls13Only` | 仅 TLS 1.3 | `min=max=TLS 1.3`。 |

选择"自动"外加"仅 X"两档可覆盖绝大多数排障/合规场景。不提供"TLS 1.3+""1.2-1.3 自选"等组合(YAGNI)。

## 3. 数据模型

`crates/iec104sim-core/src/master.rs`:

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TlsVersionPolicy {
    #[default]
    Auto,
    Tls12Only,
    Tls13Only,
}

pub struct TlsConfig {
    pub enabled: bool,
    // ... 其余已有字段 ...
    #[serde(default)]
    pub version: TlsVersionPolicy,
}
```

字段 `version` 带 `#[serde(default)]`,旧 `PersistedMasterState` JSON(<= v1.0.5)中无此字段时反序列化为 `Auto`,零迁移。

## 4. TLS Connector 行为

`create_tls_stream` 改为按 `version` 分支:

```rust
match self.config.tls.version {
    TlsVersionPolicy::Auto => {
        builder.min_protocol_version(Some(native_tls::Protocol::Tlsv12));
    }
    TlsVersionPolicy::Tls12Only => {
        builder.min_protocol_version(Some(native_tls::Protocol::Tlsv12));
        builder.max_protocol_version(Some(native_tls::Protocol::Tlsv12));
    }
    TlsVersionPolicy::Tls13Only => {
        builder.min_protocol_version(Some(native_tls::Protocol::Tlsv13));
        builder.max_protocol_version(Some(native_tls::Protocol::Tlsv13));
    }
}
```

### 平台行为说明

- **OpenSSL 后端(Linux)**: `native-tls` 直接映射到 `SslVersion::{TLS1_2, TLS1_3}`,三种 policy 行为精确,集成测试全部通过。
- **SChannel 后端(Windows 10 1903+)**: TLS 1.3 原生支持,行为与 Linux 一致。
- **macOS Security Framework 后端**: `native-tls` 0.2.18 客户端 TLS 1.3 实现不可靠 — 自签场景下 `Tls13Only` 握手会返回 SSL alert `illegal_parameter`,无论是否 `accept_invalid_certs`。原因: SecureTransport 的 TLS 1.3 客户端支持已被 Apple 弱化,推荐迁移到 Network.framework(native-tls 未使用)。`Auto` 和 `Tls12Only` 在 macOS 上均正常工作;`Tls13Only` 在生产环境对接真实服务器时效果取决于对端实现。集成测试 `master_tls13_only_handshakes_with_tls13_server` 在 Apple 平台上标记为 `#[ignore]`,但配置写入路径 (`builder.min/max_protocol_version`) 仍是正确的 — 一旦底层库/平台修复,功能即刻可用,无需代码改动。

## 5. UI(master-frontend)

位置: `master-frontend/src/components/Toolbar.vue` 新建连接弹窗,"启用 TLS" 勾选之后、证书路径字段之前新增一行:

```vue
<label class="form-label" v-if="newConnForm.use_tls">
  TLS 版本
  <select v-model="newConnForm.tls_version" class="form-input">
    <option value="auto">自动</option>
    <option value="tls12_only">仅 TLS 1.2</option>
    <option value="tls13_only">仅 TLS 1.3</option>
  </select>
</label>
```

`newConnForm` 初值增加 `tls_version: 'auto'`。仅当 `use_tls=true` 时可见,与现有证书路径字段的可见条件一致。

## 6. Tauri 桥接

`crates/iec104master-app/src/commands.rs`:

`CreateConnectionRequest` 增加 `pub tls_version: Option<String>`(snake_case 匹配 serde rename)。在 `create_connection` 里解析:

```rust
let version = match request.tls_version.as_deref() {
    Some("tls12_only") => TlsVersionPolicy::Tls12Only,
    Some("tls13_only") => TlsVersionPolicy::Tls13Only,
    _ => TlsVersionPolicy::Auto, // 缺省/未知 → Auto
};
let tls = TlsConfig { /* ... */, version };
```

策略: 未知字符串不报错,静默回退到 `Auto`,避免 UI/后端字符串不一致时直接阻断连接创建。

## 7. 错误处理

- **握手失败**(对端不支持选定版本): `create_tls_stream` 已返回 `MasterError::TlsError("TLS 握手失败: {e}")`,UI 通过 `showAlert` 弹出。无需新增分支。
- **非法配置**(比如用户改持久化 JSON 写了不认识的值): serde 因 `#[serde(other)]` 不存在会失败;为稳健起见,`version` 字段靠 `#[serde(default)]` 容错 — 整个 TlsConfig 反序列化时若 `version` 不是合法 variant,serde 会报错。若后续发现旧文件兼容性问题,可加自定义 `Deserialize` 实现;但 v1.0.5 及之前无该字段,当前方案已够。

## 8. 测试

新增 `crates/iec104sim-core/tests/tls_version_negotiation.rs`,覆盖:

1. `test_master_auto_vs_tls13_slave` — slave min=max=1.3,master `Auto` → 握手成功。
2. `test_master_tls12_only_vs_default_slave` — slave 默认(允许 1.2-1.3),master `Tls12Only` → 握手成功,且实际会话为 1.2(可通过读取证书/协议字符串验证;native-tls 不直接暴露协商版本,改为通过行为验证 STARTDT 往返即可)。
3. `test_master_tls13_only_vs_default_slave` — slave 默认,master `Tls13Only` → 握手成功。
4. `test_master_tls12_only_vs_tls13_only_slave_fails` — slave min=max=1.3,master `Tls12Only` → 握手失败,返回 `MasterError::TlsError`。

现有 `tls_e2e.rs` 保持不变(默认 `Auto` 路径等价当前行为)。

## 9. 版本与发布

- 语义: 新增字段 + 新增枚举 + 新增 UI 下拉;不破坏现有 API/配置文件。→ **patch bump v1.0.6**(仅在 `/release` 时执行)。
- CHANGELOG 条目归类: `新增 — 主站: 新建连接时可选择 TLS 版本...`。

## 10. 不做

- Slave 侧同名选项(用户只要求主站;slave 改动另开 spec)。
- "TLS 1.3+""任意 min-max 组合"等混合档(需要时再扩枚举)。
- 连接建立后动态切换版本(`TlsConfig` 目前就不可变,保持)。
- 在 About 对话框展示"当前会话协商到的 TLS 版本"(native-tls 不暴露,非目标)。
