# Ayiou WebUI Control Plane Design

## 背景与目标

Ayiou 目前具备插件元数据、消息分发、基础指标抽象，但缺少统一的 Web 管理入口。目标是引入中心化 Control Plane + Bot Agent 架构，使用户可通过 WebUI 对多个 bot 实例执行运行时管理，并保证所有操作无需重启 bot 即可生效。

核心目标：
- 启停 bot（运行时控制）
- 插件启用/禁用（运行时热生效）
- 插件配置编辑（运行时热更新）
- 日志/指标查看（按 bot 与插件维度）

## 范围

- 新增独立 Control Plane 服务（WebUI + Admin API + RBAC）
- 每个 bot 进程新增 Agent 侧控制能力
- 通过长连接控制通道实现命令下发、状态回传、日志指标上报
- 配置存储抽象 `ConfigStore`，支持 `toml`（默认）/`sqlite`/`redis`/`postgres`
- V1 提供插件调度层开关；V2 提供 Wasm 动态加载/卸载

## 不做事项（V1）

- 不在 V1 实现原生动态链接库（`dlopen`）热插拔
- 不在 V1 实现跨地域高可用控制平面
- 不在 V1 实现复杂多租户隔离

## 总体架构

### 组件

1. Control Plane（独立服务）
- 提供 WebUI、REST API、RBAC、审计日志
- 管理 bot 实例注册、在线状态、命令下发
- 聚合日志与指标供 UI 查询

2. Bot Agent（每个 bot 进程内）
- 与 Control Plane 建立双向长连接
- 接收并执行管理命令（start/stop/enable/disable/config-update）
- 上报心跳、状态、日志、指标

3. Plugin Runtime（bot 内）
- V1：插件调度层开关（立即生效）
- V2：Wasm 插件运行时（命令/regex/cron）+ 热加载/卸载

4. Config Store（可插拔后端）
- 统一接口，后端实现：`toml`、`sqlite`、`redis`、`postgres`
- 用户按部署需求选择，默认 `toml`

### 控制通道

使用双向流式协议（gRPC stream 或 WebSocket）承载：
- 命令下发
- 命令执行回执
- 状态/心跳
- 日志/指标事件

协议要求：
- 每条命令携带 `command_id`（幂等）
- Agent 回报 `accepted/rejected` 与最终执行状态
- Control Plane 以最终状态落审计日志

## 核心数据模型

### bot_instance
- `id`
- `name`
- `env`
- `status`（`RUNNING` / `STOPPED` / `DEGRADED`）
- `last_heartbeat_at`
- `agent_version`

### plugin_state
- `bot_id`
- `plugin_name`
- `enabled`
- `mode`（`DISPATCH_GATE` / `WASM`）
- `updated_at`

### plugin_config
- `bot_id`
- `plugin_name`
- `version`
- `content`
- `store_backend`（`toml` / `sqlite` / `redis` / `postgres`）
- `checksum`

### RBAC
- `rbac_user`
- `rbac_role`
- `rbac_binding`

权限最小粒度：
- `bot:start`
- `bot:stop`
- `plugin:enable`
- `plugin:disable`
- `config:read`
- `config:write`
- `logs:read`
- `metrics:read`

### audit_log
- 操作人
- 操作目标（bot/plugin）
- 操作类型
- 请求摘要
- 执行结果
- `trace_id`
- 时间戳

## Admin API（WebUI 调用）

- `POST /api/v1/auth/login`
- `GET /api/v1/bots`
- `POST /api/v1/bots/{id}/start`
- `POST /api/v1/bots/{id}/stop`
- `GET /api/v1/bots/{id}/plugins`
- `POST /api/v1/bots/{id}/plugins/{name}/enable`
- `POST /api/v1/bots/{id}/plugins/{name}/disable`
- `GET /api/v1/bots/{id}/plugins/{name}/config`
- `PUT /api/v1/bots/{id}/plugins/{name}/config`
- `GET /api/v1/bots/{id}/logs?level=&since=`
- `GET /api/v1/bots/{id}/metrics`

## 运行时热更新机制

### 1) Bot 启停（不退出进程）

- `START_BOT`：启动 adapter 收包与分发任务
- `STOP_BOT`：停止收包并关闭分发任务，保留进程和控制通道
- 同状态重复请求返回幂等成功

### 2) 插件启用/禁用（V1）

- 引入 `PluginRuntimeState`（并发 map）保存 `plugin_name -> enabled`
- 分发前读取开关，禁用插件直接跳过
- Agent 原子更新状态后立即生效

### 3) 插件配置热更新

- Agent 持有 `ConfigRegistry`（当前生效版本 + checksum）
- `UPDATE_PLUGIN_CONFIG`：校验 -> 持久化 -> 发布到运行时 -> 插件回调
- 插件 apply 失败时保持旧配置并返回失败原因（支持回滚）

### 4) 日志/指标

- Agent 将日志与指标标准化为事件并上报 Control Plane
- WebUI 提供按 bot/plugin 过滤与时间窗口查询
- 复用现有指标：`events_in_total`、`plugin_errors_total`、`plugin_handle_duration_ms`

## V2：Wasm 动态加载/卸载

V2 明确采用 Wasm 作为动态插件形态，不采用 `dlopen`。

### 目标能力
- `command + regex + cron`
- 运行时加载/卸载 `.wasm` 模块，不重启 bot
- 版本切换与失败回滚

### 运行时选型
- 推荐 `wasmtime` 托管模块
- 定义稳定 Plugin ABI（按版本演进）
- Host API 提供：
  - 消息发送
  - 配置读取与更新通知
  - 日志上报
  - 调度注册（cron）
  - 存储访问（受限能力）

### 安全与资源限制
- 限制 CPU 时间片、内存、超时
- 控制可访问能力（capability-based）
- 记录模块级审计事件

## 错误处理策略

- 控制命令执行失败：返回明确错误码与错误信息
- Agent 掉线：bot 状态标记 `DEGRADED`
- 配置校验失败：拒绝应用并保留旧版本
- 插件异常：只影响对应插件，不拖垮整个分发流程

## 测试策略

- 单元测试
  - RBAC 权限判定
  - 插件状态机幂等与并发更新
  - `ConfigStore` 多后端契约一致性
- 集成测试
  - Control Plane <-> Agent 命令链路
  - start/stop 与 enable/disable 热生效
  - 配置热更新成功/失败回滚
- E2E
  - 登录 -> 管理 bot -> 插件开关 -> 配置编辑 -> 日志指标查看
- Wasm 专项（V2）
  - `command/regex/cron` 行为回归
  - 模块热替换 + 回滚
  - 资源限制有效性

## 里程碑

- M1：Control Plane 基线
  - 多用户 + RBAC
  - Bot 注册、心跳、状态
  - Start/Stop bot
- M2：插件管理 + 配置热更新
  - 插件启用/禁用（调度开关）
  - 配置中心（`toml` 默认 + 其余后端）
  - 审计日志
- M3：日志与指标
  - 日志聚合查询
  - 指标聚合展示
- M4：Wasm 动态插件
  - Wasm ABI + Host API
  - command/regex/cron
  - 热加载/卸载 + 回滚

## 风险与控制

- 风险：Wasm ABI 范围过大导致后续演进困难
  - 控制：先冻结最小 Host API，以版本化方式扩展
- 风险：多后端配置一致性偏差
  - 控制：统一契约测试，后端实现必须通过同一测试套件
- 风险：控制命令幂等缺失导致重复执行
  - 控制：强制 `command_id` 去重与状态机校验
