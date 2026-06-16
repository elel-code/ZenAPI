# ZenAPI 开发 TODO

> 最后更新: 2026-06-14
> 本文档是 ZenAPI 的开发路线图和任务清单，按阶段组织。
> 已完成的项标记为 `[x]`，进行中的标记为 `[~]`，待完成的标记为 `[ ]`。

---

## 阶段 0: UI 与功能调研 【重点·优先】

> **目标**: 通过实时联网调研 5 个主流 API 客户端的 UI 样式、交互模式和功能集，
> 形成参考文档，为 ZenAPI 的 UI 和功能设计提供参照。
> **本阶段是后续所有 UI 决策和功能设计的参照基础，应在各阶段实现前优先完成。**
> **其中 0.1（布局视觉）和 0.2（交互模式）是核心中的核心——UI 参照比功能/架构参照更直接影响产品体验，必须最优先完成。**
>
> **调研对象**:
> | 客户端 | 定位 | 技术栈 | 许可证 | 调研侧重 |
> |--------|------|--------|--------|----------|
> | **Postman** | 行业标杆，全生命周期平台 | Electron + React | 专有 | 功能完备度上限 |
> | **Hoppscotch** | 开源，Web/PWA 优先 | Vue.js + Nuxt + TS | MIT | 低摩擦体验、多协议 |
> | **Bruno** | Git-native，本地文件存储 | Electron + JS | MIT | 离线优先、Bru 标记语言 |
> | **Insomnia** | Kong 维护，设计+调试一体 | Electron + React | Apache 2.0 | OpenAPI 原生、深色主题 |
> | **Yaak** | 隐私优先，Rust 原生桌面 | Tauri + Rust + React | 专有(源码可用) | 最近技术参照 |
>
> **设计参考**: 综合 Postman 的功能深度、Hoppscotch 的低摩擦请求体验、Bruno 的文件存储和 Yaak 的原生桌面体验——**目标为功能完备、隐私优先、本地优先的原生 API 工作站**。
>
> **当前状态**: 已补充离线基线文档，用于固化 GPUI 重构、Linux `gpui_platform`、依赖升级策略和无 bundled font 资产等方向；实时联网截图和版本号仍待复核。

### 0.1 布局与视觉系统调研 【UI 核心·最高优先】
- [ ] 0.1.1 调研 Postman 布局: Header / Sidebar / Workbench(Tabs) / Footer / Right Sidebar 五区模型
- [ ] 0.1.2 调研 Hoppscotch 布局: 可折叠侧边栏 + 单页请求区、响应式断点行为
- [ ] 0.1.3 调研 Bruno 布局: 集合树 + 请求编辑 + 响应面板，Bru 文本视图
- [ ] 0.1.4 调研 Insomnia 布局: 左侧边栏 + 中央请求构建器 + 右侧文档面板 + 响应底部
- [ ] 0.1.5 调研 Yaak 布局: Tauri 原生框架、标签页管理、主题切换
- [ ] 0.1.6 横评 5 个客户端的配色方案: 浅色/深色主题、表面色、边框色、强调色、HTTP 方法色映射
- [ ] 0.1.7 横评字体与排版: 正文字体、代码字体、字号层级、行高、字间距
- [ ] 0.1.8 横评控件样式: 按钮(主要/次要/危险/禁用)、输入框、下拉菜单、标签页、开关
- [ ] 0.1.9 横评图标体系: 图标风格(Material/自定义)、尺寸、用途分类
- [ ] 0.1.10 横评响应查看器: Pretty/Raw/Preview/Headers 的视觉对比和切换
- [~] 0.1.11 输出 UI 视觉调研文档: `docs/02_DESIGN.md` 离线基线已输出，实时截图和版本状态待复核

### 0.2 交互模式调研 【UI 核心·最高优先】
- [ ] 0.2.1 侧边栏交互横评: 集合树展开/折叠、拖拽排序、右键菜单、搜索过滤、收藏
- [ ] 0.2.2 请求构建器交互横评: 方法选择器、URL 输入、Params/Headers/Body 编辑区切换
- [ ] 0.2.3 标签页系统横评: Postman 的多标签 vs Hoppscotch 的单页 vs Yaak 的原生标签
- [ ] 0.2.4 双栏/单栏视图: Postman 的 Two Pane View、各客户端响应区布局策略
- [ ] 0.2.5 键盘快捷键体系横评
- [ ] 0.2.6 PWA/离线体验: Hoppscotch 的 Service Worker 策略启发 ZenAPI 原生离线
- [~] 0.2.7 输出交互调研文档: `docs/02_DESIGN.md` 离线基线已输出，实时行为复核待补齐

### 0.3 核心功能对比矩阵
- [ ] 0.3.1 Collections 对比: 组织结构(集合→文件夹→请求)、导入导出、Postman Collection v2.1 兼容性
- [ ] 0.3.2 Environments & Variables 对比: 作用域(全局/集合/环境)、`{{var}}` 语法、Bruno 的 Bru 变量
- [ ] 0.3.3 Pre-request Scripts 对比: 执行时机、可用 API、Hoppscotch 的简化脚本
- [ ] 0.3.4 Tests 对比: 断言语法、测试结果展示、Bruno 的 CLI 运行器
- [ ] 0.3.5 Authorization 对比: 认证类型覆盖度(Bearer/Basic/OAuth2/API Key/Digest/JWT)
- [ ] 0.3.6 Request Body 对比: form-data/x-www-form-urlencoded/raw/binary/GraphQL 编辑
- [ ] 0.3.7 Headers & Params 对比: 批量编辑、自动补全、Insomnia 的 Header 预设
- [ ] 0.3.8 Mock Server 对比: Postman Mock vs Insomnia Mock vs ZenAPI 本地 Mock
- [ ] 0.3.9 Code Generation 对比: 语言覆盖度(所有客户端均支持 cURL + 多语言)
- [ ] 0.3.10 History 对比: 历史保存策略、搜索、Yaak 的本地加密存储
- [ ] 0.3.11 多协议支持对比: REST / GraphQL / WebSocket / gRPC / SSE / MQTT / Socket.IO
- [ ] 0.3.12 自托管/离线能力对比: Hoppscotch Docker 自托管、Bruno 本地文件系统、Yaak 完全本地
- [ ] 0.3.13 AI 辅助能力对比: Postbot / Hoppscotch AI / Insomnia AI / ZenAPI 探索
- [ ] 0.3.14 调研 Bruno 的 Bru 纯文本标记语言: 语法设计、Git 友好性、人类可读性
- [ ] 0.3.15 调研 Yaak 的插件系统: 认证插件、模板标签、UI 定制扩展点
- [ ] 0.3.16 调研 Insomnia 的 OpenAPI 编辑器: 可视化预览、设计模式
- [x] 0.3.17 功能对比矩阵已归档，不再作为设计参照。

### 0.4 技术架构调研
- [ ] 0.4.1 横评技术栈: Electron(Postman/Bruno/Insomnia) vs Vue+PWA(Hoppscotch) vs Tauri+Rust(Yaak) vs GPUI+Rust(ZenAPI)
- [ ] 0.4.2 横评离线、本地文件、插件/扩展和跨平台发布模型
- [~] 0.4.3 输出技术架构调研文档: `docs/02_DESIGN.md` 离线基线已输出，实时版本复核待补齐

### 0.5 调研总结与优先级调整
- [~] 0.5.1 汇总调研发现，明确 ZenAPI 的优势方向: 原生 Mock Server、GPUI 原生体验、OpenAPI 深度集成（离线基线已写入）
- [~] 0.5.2 识别当前功能缺口: 多协议(WebSocket/gRPC)、插件系统、AI 辅助（离线基线已写入）
- [ ] 0.5.3 基于调研结果调整本文档后续阶段的优先级排序

---

## 阶段 1: GPUI 应用外壳重写

> **目标**: 替换现有 Slint 原型代码，建立 GPUI 应用架构基础。
> 遵循 PRD 中的兼容性策略：GPUI 重写是破坏性替换，不保留 Slint UI 文件。
> 颜色、间距等 UI 基准由本阶段 1.2 主题系统定义，视觉参照以阶段 0 调研为补充。

- [x] 1.1 搭建 GPUI 应用入口 (`main.rs`)，通过 Zed 官方 `gpui_platform::application()` 启动；不再保留 bundled font 资产
- [x] 1.2 实现全局 UI 主题系统: 颜色 token、HTTP 方法色映射、状态色调映射（参考 Insomnia 深色主题的色调层次）
- [x] 1.3 实现顶部全局控制栏: 品牌区、规格状态指示、Import 路径输入和按钮
- [x] 1.4 实现左侧边栏框架: 固定宽度、路由列表容器、空状态占位（参考 Bruno 简洁侧边栏风格）
- [x] 1.5 实现主工作区布局: 请求地址栏、请求编辑区、响应查看区
- [x] 1.6 实现底部状态栏: Mock 服务器状态、端口显示、全局状态指示
- [x] 1.7 实现标签页系统: 40px tab-style 标题行、激活下划线、请求/响应内容区
- [x] 1.8 清理所有 Slint 遗留文件 (`ui/` 目录、Slint 构建脚本、生成的 UI 模块)
- [x] 1.9 验证: `cargo check && cargo test` 通过

---

## 阶段 2: OpenAPI / Swagger 引擎集成

> **目标**: 将现有的 OpenAPI 解析模块接入 GPUI 应用。

- [x] 2.1 将 `src/openapi/` 模块适配 GPUI 数据流（状态所有权、事件模型）
- [x] 2.2 实现 Import 控制栏 UI: 文件路径输入、Enter/按钮确认、导入进度状态
- [x] 2.3 实现路由列表渲染: 方法标记（固定宽度文本，非填充徽章）、路径文本、可选摘要行
- [x] 2.4 实现路由选中态: 3px 主色左边标记 + 行背景高亮
- [x] 2.5 实现路由过滤: 按方法/路径/摘要过滤，过滤不影响全局操作
- [x] 2.6 实现导入新规格时自动停止运行中的 Mock 服务
- [x] 2.7 验证: JSON/YAML 解析与磁盘文件导入测试覆盖

---

## 阶段 3: API 客户端核心

> **目标**: 实现完整的 HTTP 请求发送和响应展示功能，追求完备的编辑器体验和流畅的操作感。

- [x] 3.1 实现请求方法选择器: GET/POST/PUT/PATCH/DELETE/OPTIONS/HEAD，下拉或分段控件
- [x] 3.2 实现 URL 地址栏: 单行输入、Enter 发送、与路由选择联动
- [x] 3.3 实现请求 Headers 编辑面板: 固定键值对表格、常用 Header 提示、剪贴板批量复制/粘贴均已支持
- [x] 3.4 实现 Params 编辑面板: 键值对表格、自动拼接到 URL
- [x] 3.5 实现 Request Body 编辑面板:
  - [x] 3.5.1 支持 `none` / `form-data` / `x-www-form-urlencoded` / `raw` / `binary` 类型切换
  - [x] 3.5.2 `raw` 模式下支持 JSON/XML/Text/HTML 内容类型切换，并提供 JSON/XML/HTML 基础语法高亮预览（`syntect` 保留为未来增强选项）
  - [x] 3.5.3 `form-data` 支持文本字段和 `@file` 文件附件
- [~] 3.6 实现 Authorization 面板:
  - [x] 3.6.1 Bearer Token
  - [x] 3.6.2 Basic Auth
  - [x] 3.6.3 API Key (Header/Query)
  - [~] 3.6.4 OAuth 2.0: 手动 access token 已支持；授权码/PKCE、redirect、refresh 与安全状态存储仍为远期
  - [x] 3.6.5 JWT
- [x] 3.7 实现发送请求: 通过 `reqwest` 发送，显示加载状态，按钮禁用防重入
- [x] 3.8 实现响应查看器:
  - [x] 3.8.1 状态码与响应摘要元数据显示（右对齐，180px 上限，14px 右内边距）
  - [x] 3.8.2 响应体 Pretty 模式: JSON 格式化、缩进与折叠摘要均已支持
  - [x] 3.8.3 响应体 Raw 模式: 原始文本
  - [x] 3.8.4 Response Headers 子标签页
  - [x] 3.8.5 响应查看器使用平台 monospace 样式，并通过专用只读可选中文本组件避免编辑光标
- [x] 3.9 验证: 本地 Axum HTTP 请求测试覆盖状态码、响应头、Raw body 与 Pretty body
- [~] 3.10 UI 对齐检查: Headers/Body/Response 面板间距一致性、控件颜色使用共享 UI 辅助函数（非硬编码）、HTTP 方法色映射统一；已将 HTTP 方法标签宽度、平台 monospace、主工作区 surface/border/text/accent 色和核心列表 metric 收敛到共享常量/辅助函数，视觉参照 `docs/02_DESIGN.md` 调研横评仍需截图复核

---

## 阶段 4: 本地 Mock 服务器

> **目标**: 将现有的 Mock 服务器模块接入 GPUI，增强可用性。
> 原生 Mock 服务器是 ZenAPI 的核心特性之一。

- [x] 4.1 将 `src/mock_server/` 模块适配 GPUI 数据流
- [x] 4.2 实现 Mock 开关控件: 启动/停止按钮、端口显示、启用/禁用状态
- [x] 4.3 实现 CORS 默认开启，无需额外配置
- [x] 4.4 增强 Mock 响应生成:
  - [x] 4.4.1 基于 OpenAPI Schema 的 JSON 响应生成
  - [x] 4.4.2 Schema-aware 假数据生成: 字段名启发式 (`email` → 生成 email, `name` → 生成姓名)
  - [x] 4.4.3 支持响应示例 (`examples`) 优先于 Schema 生成
- [x] 4.5 实现 Mock 请求日志: 记录到达的请求，在 UI 中展示最近 N 条
- [x] 4.6 验证: 启动 Mock 服务，OPTIONS 预检与 JSON 响应均包含 CORS 头

---

## 阶段 5: 环境与变量系统

> **目标**: 实现环境配置和变量替换。

- [x] 5.1 设计变量数据模型: 变量名、当前值、初始值、作用域（全局/环境），参考 Bruno 的文件系统变量
- [x] 5.2 实现变量解析引擎: `{{variableName}}` 语法替换
- [x] 5.3 实现环境管理 UI:
  - [x] 5.3.1 环境列表与创建/删除：支持 dev/test/prod 初始环境、任意环境创建、当前环境删除、切换时保留各环境变量
  - [x] 5.3.2 环境变量编辑器（键值对表格）
  - [x] 5.3.3 当前活跃环境指示
- [x] 5.4 实现全局变量管理 UI
- [x] 5.5 变量集成到请求构建器: URL/Headers/Body 中均支持 `{{var}}` 替换
- [x] 5.6 验证: 创建环境变量，在请求 URL 中使用，确认替换正确
- [~] 5.7 UI 对齐检查: 变量编辑器键值表格列宽对齐、行高一致、环境选择器下拉与全局变量面板控件风格统一、删除按钮/新增行交互反馈一致；键值表格主列宽已抽成共享 UI metric 并增加回归测试，视觉参照 `docs/02_DESIGN.md` 调研横评仍需截图复核

---

## 阶段 6: 集合系统

> **目标**: 实现集合系统，支持请求的组织、存储和复用。
> 集合数据以纯文本 JSON 存储在本地文件系统，天然 Git 友好。

- [x] 6.1 设计集合数据模型: 集合 → 文件夹 → 请求的三级结构
- [x] 6.2 实现集合序列化/反序列化 (JSON 格式，兼容 Postman Collection v2.1 格式)
- [x] 6.3 Bru-style 探索已归档（BRU_FORMAT.md 已删除），当前保留原生 JSON，后续优先做 Bru-style export
- [x] 6.4 实现集合侧边栏:
  - [x] 6.4.1 集合树渲染: 展开/折叠
  - [x] 6.4.2 右键菜单: 新建请求/文件夹、重命名、删除、复制
  - [x] 6.4.3 请求拖拽移动
- [x] 6.5 实现请求保存: 从当前请求构建器保存到集合
- [x] 6.6 实现集合导入/导出: 兼容 Postman Collection JSON 格式
- [x] 6.7 验证: Postman Collection JSON 模型导入/导出与集合树可见行层级均有自动化测试覆盖
- [~] 6.8 UI 对齐检查: 集合树节点间距/缩进一致、展开折叠图标尺寸固定、右键菜单样式与系统其他菜单一致、拖拽排序视觉反馈平滑、空文件夹/空集合占位与侧边栏空状态风格统一；集合树行高/缩进/图标宽度/拖拽色已抽成共享 UI metric 并增加回归测试，交互参照 `docs/02_DESIGN.md` 调研横评仍需截图复核

---

## 阶段 7: 请求历史

> **目标**: 自动保存请求历史，支持回溯和复用。

- [x] 7.1 设计历史记录数据模型: 时间戳、请求详情、响应摘要
- [x] 7.2 实现请求自动记录: 每次发送请求后自动保存
- [x] 7.3 实现历史侧边栏视图: 按时间倒序、搜索过滤
- [x] 7.4 实现历史条目操作: 点击恢复请求、删除单条/批量清除
- [x] 7.5 验证: 自动记录、过滤/恢复模型、历史侧边栏过滤可见行与无匹配空状态均有自动化测试覆盖

---

## 阶段 8: 代码生成

> **目标**: 支持多语言请求代码片段生成。

- [x] 8.1 设计代码生成器架构: 语言模板 + 请求数据填充
- [x] 8.2 实现首批语言支持: cURL / Python (requests) / JavaScript (fetch) / Rust (reqwest) / Go (net/http)
- [x] 8.3 实现代码片段 UI: 语言选择下拉、代码预览、一键复制
- [x] 8.4 验证: 构建请求，生成 cURL 命令，在终端执行验证

---

## 阶段 9: 集合运行器

> **目标**: 批量执行集合请求，支持顺序/并行调度。

- [x] 9.1 设计运行器调度模型: 顺序执行、请求间延迟、继续/遇错停止失败策略已实现；并行执行保留为后续增强
- [x] 9.2 实现运行器 UI: 当前集合 Run、Stop Fail 切换、执行状态与结果摘要已接入 GPUI
- [x] 9.3 实现 CLI 运行器: `zenapi run collection.json` 命令行入口，支持 `--delay-ms` 与 `--stop-on-failure`
- [x] 9.4 验证: 本地 Axum 集合运行测试覆盖成功/失败汇总、继续执行与遇错停止；CLI 帮助路径已验证

---

## 阶段 10: 脚本与测试（远期）

> **目标**: 支持 Pre-request Scripts 和 Tests 脚本，实现请求前置处理和响应断言。

- [x] 10.1 评估 Rust 嵌入脚本引擎方案: 已输出 `docs/08_SCRIPTING.md`，建议先定义宿主 API/结果模型，优先 Rhai，小心评估 `deno_core` 的权限模型和集成复杂度
- [~] 10.2 实现 Pre-request Script 编辑器和执行: 已实现原生 script-lite action line，支持 method/url/header/query/body/变量覆盖，并支持删除 header/query/变量覆盖；单次发送、代码生成、GPUI Runner 与 CLI Runner 共用执行路径；保存集合时保留 raw 请求字段和 action line，执行时再应用；完整脚本引擎、多行编辑器和执行日志待实现
- [~] 10.3 实现 Tests 脚本编辑器和断言 API: 已实现原生 Tests 编辑器与 Rust 响应断言模型（状态码、Header、Body、JSON Path），支持保存/恢复到集合请求；`pm.test` / `pm.expect` 脚本兼容层待实现
- [~] 10.4 实现测试结果展示面板: 单次请求 Tests 面板、Runner 结果行与响应摘要均已展示断言通过/失败详情；Pre-request 面板已显示最近 action 执行数量、构建错误和 action 名称/目标明细；GPUI Runner 摘要与 CLI 输出已记录 pre-request action 名称/目标；完整脚本引擎运行日志待实现
- [~] 10.5 验证: 响应断言编辑器字段解析、断言成功/失败、预期 500、JSON Path 失败、runner 汇总、pre-request script-lite 执行与 pre-request 构建失败路径均有自动化测试覆盖；完整脚本引擎执行验证待实现

---

## 阶段 11: 多协议支持（远期）

> **目标**: 扩展协议支持，覆盖 GraphQL、WebSocket、SSE 和 gRPC。

- [~] 11.1 GraphQL 支持: GraphQL 请求体模式已接入 query/variables 编辑并生成标准 `application/json` 请求体，内置 introspection 查询填充、introspection 响应 schema 摘要、根字段浏览、类型索引、directive 列表和根 Query 模板应用已实现；完整字段选择器/查询辅助仍待实现
- [x] 11.2 WebSocket 支持: 基于 `tokio-tungstenite` 的 `ws://`/`wss://` 持久连接、连接级 headers/subprotocols、Text/Hex 消息多次发送/接收、Open/End 显式连接控制、GPUI 消息面板、消息历史复制/清空和本地 echo/handshake 会话测试已实现
- [x] 11.3 SSE (Server-Sent Events) 支持: 基于 `reqwest` stream 的 `text/event-stream` 事件解析、最多 N 条事件采集、Once/Stream 自定义 headers、长连接后台订阅、手动停止、Last-Event-ID 续订、自动重连/backoff、GPUI SSE 面板、事件日志复制/清空和本地 Axum SSE 流/headers/重连测试已实现
- [~] 11.4 gRPC 支持评估: 已输出 `docs/09_GRPC.md`，明确 `tonic` + `prost-reflect` + `tonic-reflection` 路线、运行时 descriptor 加载、unary MVP 与 streaming 后续拆分；domain model、descriptor 加载和 unary 传输层仍待实现

---

## 阶段 12: 插件与扩展系统（远期）

> **目标**: 实现插件系统，开放认证、模板标签和 UI 定制能力。

- [ ] 12.1 设计插件加载模型: 沙箱、生命周期、权限控制
- [ ] 12.2 实现认证插件接口: 自定义认证类型注册
- [ ] 12.3 实现模板标签插件: 自定义 `{{tag}}` 处理器
- [ ] 12.4 设计 UI 主题扩展点
- [ ] 12.5 验证: 加载自定义插件，认证和模板标签正常工作

---

## 阶段 13: 打磨与发布

> **目标**: UI 细节对齐与调研参照、稳定性、错误处理和文档完备，准备首次发布。
>
> **UI 持续优化原则**: 本阶段是对前序各阶段 UI 对齐检查（3.10/5.7/6.8）的汇总校验。
> 每个功能模块应在实现阶段当即完成 UI 对齐，不把 UI 债堆积到最后一次性修。

### 13.1 UI 系统完整性审查（参照: `docs/02_DESIGN.md`）
- [~] 13.1.1 颜色审查: 全局表面色/边框色/强调色、warning/status、HTTP 方法色、syntax 色和文本选择色已收敛到共享 token，并移除 app/input/read-only text 渲染路径中的裸 RGB；Sidebar/Request/Response pane、Request/Response tab bar 与 muted control fill 已统一为白色 surface，top/status chrome、hover 与 workspace gutter 保持纯中性灰，避免 pane 背景读成多色块；全局 secondary/muted、placeholder、sidebar detail 与 strong border token 已拆分，sidebar route summary、History status 与 saved request URL 使用 body contrast，placeholder 调整为 `#b8c0cc`、strong border 调整为 `#aeb7c2`，避免输入框边界和占位文字过浅；选中路由、Collection drag-over 和 primary/warning 按钮已移除大面积彩色铺底，保留细强调线/中性 hover/语义边框文字；截图级视觉复核仍待完成
- [~] 13.1.2 字体排版审查: TextInput 已复用平台 UI/monospace 字体 token，正文/代码路径继续使用共享字体常量；Request pane section title、表格头、preview 标题、Request/Response tab 非激活文本、toggle 默认文本、Scripts 结果行、Realtime 日志行和 Sidebar detail 行已从 muted/secondary 提升到 primary/body；TextInput placeholder 已切到独立 token，normal/disabled 空输入保持同一中等偏浅灰，避免占位内容过浅或压过真实输入；Route filter、Tests assertion、Pre-request、GraphQL vars、WebSocket/SSE setup 等高频空输入 placeholder 已收敛为短字段名，减少空表单文字噪音；pane header title 已提升到 19px，局部 panel 标题、TextInput、Request method/Send 主控件、Response body、Raw/GraphQL preview、Code snippet、Tests/Pre-request 结果行和 Runner/Mock/Realtime 日志正文提升到 18px，并将对应正文/代码行高和 TextInput 行高提升到 26px；Request tabs、sidebar nav、左栏主行、sidebar 方法标签、通用 action button、表头、panel meta、sidebar action、compact toggle、行 meta 与状态/计数保持 16px，top-bar action 提升到 15px 但仍低于主控件，compact 方法/status cell 提升到 15px；GPUI app shell 中的 `text_size(px(...))` 已全部收敛为命名字号 token，避免后续继续散落裸数字；Request pane idle/configured 状态与常见空状态保持 body 级对比度；各面板视觉复核仍待完成
- [~] 13.1.3 间距与尺寸审查: 窗口、应用级顶栏、左/中/右三 pane 工作台、Request pane 内组合式请求地址栏 54px/40px、Request editor 34px tab bar、Raw/GraphQL body editor 118px、GraphQL vars editor 86px、Body/GraphQL preview 86px、GraphQL schema browser 112px、可调整 sidebar/request/response split、Sidebar pane 0.24 最小比例、Request pane 0.32 最小比例、Response pane 0.26 最小比例、split 48px 预览步长、左栏 42px 三段导航、响应 Tests meta 180px 上限/14px、底栏右侧状态 220px 上限、Tests kind 96px、Tests Clear 54px、选中路由 3px、panel header、empty state、Import 弹出层 520x58、method 菜单、通用 key/value editor 128px key 列、Body 字段表 112px key 列、key/value 行操作按钮 30px、key/value 与 Tests 表头语义标签、细窄滚动条 6px thumb/12px gutter 与 20px 内容右侧安全区尺寸已 token 化并有回归测试；method menu item、sidebar 小按钮、compact control 和 sidebar section button 统一到 30px，top-bar action、status bar、code snippet box/line height、Response body line height、TextInput line height、History row、Request address divider、hidden scrollbar placeholder、Sidebar 二级行缩进、Panel header underline offset、compact toggle 宽度规则和 collection drag preview 尺寸也已 token 化；字号提升后 collection tree root/folder/request 行分别上调到 32/32/42px，result/log row 上调到 34px，empty row 上调到 36px，避免平台字体较高时左栏树、日志和结果行文字贴边或裁切；左栏 HTTP 方法列已随字号提升放宽到 68px，避免 `OPTIONS` 等长方法在 sidebar 行内擦边；字号 token 和相邻控件高度已纳入同一 metric 回归测试；截图级视觉复核仍待完成
- [~] 13.1.4 控件样式审查: 顶栏已收敛为 72px 品牌槽、Import 和 Mock/Stop 应用级动作，Mock 详细状态保留在底栏且运行地址只显示端口，启动/停止/失败也使用短标签；Import 路径移入紧凑弹出层；左栏导航已收敛为 Routes/Saved/History 单一上下文，集合操作按钮改为弹性两行布局，Postman 导出入口收短为 `PM`，Collection 菜单动作已收敛为 `x`/`Del`/`+ Req`/`+ Dir`/`Copy`/`Rename`；请求栏 method、URL 和 Send 已合并为 Request pane 内的单个地址栏外壳，URL 使用 inline TextInput，只有 URL 聚焦会反馈到组合地址栏边框，method 菜单打开只影响 method 文本、箭头和菜单 chrome，busy 期间 URL 输入会随组合地址栏进入真实 disabled 状态并停止编辑/选区/IME 写入；TextInput shell 和组合地址栏已统一为 2px 边框，method selector 已移除整块 hover/打开态高亮，不再染色整个 method segment 或整条地址栏，Send segment 也改为中性白底加强调文字，并已补回归测试固定该规则；Request pane 已从所有编辑器纵向堆叠改为 `Params`/`Hdrs`/`Auth`/`Body`/`Script`/`Live`/`Tools` 内容 tabs，tab 内 Auth/Body/Vars/Realtime/Tests/Runner 控件已改为分行和弹性按钮，Auth 标题、模式与 Basic/API 输入已收敛为 `Auth`、`OAuth`、`API`、`User`、`Pass` 等短文案，Hdrs 面板标题保持 `Hdrs` 且列头保留 `Header`，Script 内 Pre-request 标题已收敛为 `Pre`，Codegen 语言 selector 已收敛为 `cURL`/`Py`/`JS`/`Rust`/`Go`；Body tab 已移除重复 `Body` 标题，mode toggle 直接作为首个控件并从长 content-type 文案收敛为两行三列短标签，URL 表单模式与字段编辑器标题已收敛为 `URL Enc`，Form 字段编辑器标题已收敛为 `Form`，mode/toggle 默认态也改为白底和 body 文本；Params/Hdrs/Vars/Form/URL Enc/WebSocket/SSE key/value 表格和 Tests assertion 表格已提供统一固定宽 `+`/`x` 行操作，WS/SSE header 小节标题已收敛为 `WS Hdrs`/`SSE Hdrs`，Vars 面板标题、GraphQL vars placeholder、GraphQL Templates 变量预览前缀、Env vars 表标题、变量表列名和新增行 placeholder 已收敛为 `Vars`/`Env`/`Var`/`var`，GraphQL Templates `Use` 入口也已命名为短标签，Tests 表头与 placeholder 已收敛为 `Test`/`Kind`/`Target`/`Expect`，kind selector 已收敛为 `Status =`/`Range`/`Header ?`/`Header =`/`Body ?`/`JSON =`，Tests 新增入口已从大按钮行收敛到 header `+`，结果清理只在有结果时显示为紧凑 `Clear`；Tools 内 Mock 日志标题已收敛为 `Log`，GraphQL/Code/Runner 等子面板标题以及 Send/Raw/Codegen Copy 等主控件文案已收敛为命名短标签，避免重复当前工具上下文或散落硬编码文字；sidebar/request 与 request/response divider 已变为可拖拽 resize handle，拖动时显示中性 divider preview，释放后一次性提交 pane 比例，预览目标按 48px 步长量化后才更新；按钮主色/次要/危险/禁用状态由共享 UI 辅助函数统一管理，primary/warning 按钮使用白底、语义边框和语义文字，disabled controls 已改用独立 surface/border/text token 和共享 opacity，不再复用 hover 色；app shell 里的常用圆角已收敛为 tight/control/input 三档 token，并纳入 metric 回归测试；弹出层截图复核仍待完成
- [~] 13.1.5 交互规范审查: Import/Method/Codegen/Collection 临时层、Escape 关闭、busy 关闭与入口 guard 已统一；Sidebar、Request pane 和 Response body 已改为固定头部/工具条 + 独立滚动内容区，并通过 GPUI ScrollHandle + ZenAPI 自绘细窄滚动条提供 track/thumb 交互；Request/Response tab bar、Import popover、Routes/Saved/History filter、Saved JSON path、URL/method、key/value 表格、Auth、Tests、Pre-request、Body、Realtime setup、Preview/Schema/Codegen 和底栏状态均已补 min-width/overflow/truncate 或内部滚动边界，避免长内容撑开 pane；左栏 section title 已统一为 `Routes`/`Saved`/`History`，filter placeholder 统一为 `Filter`，空状态收敛为 `No routes`/`No saved`/`No history`/`No matches`；Runner 空状态收敛为 `No requests`/`No results`，Response 初始空态与 Codegen 空 URL 预览收敛为 `No response`/`No URL`，Realtime/Mock 日志空态收敛为 `No messages`/`No events`/`No logs`；split resize 使用中性预览并在释放后提交量化比例；Routes/History 计数和底栏 route status 已改为未过滤时只显示纯总数、过滤后才显示 `visible/total`，Saved header 默认 collection 状态会隐藏，常见 collection 结果压缩为 `Imported`/`Exported`/`+ Req`/`Busy` 等短标签，默认 idle `Ready`、stopped/ready/no-route mock 和 Response `Idle` 文案不再常驻显示，运行中 Mock 地址收敛为端口短标签，底栏右侧状态槽也只在有 Response 状态或 Busy 时占位且最多占 220px，OpenAPI import 成功响应已改为 title 放 spec 名、meta 放 route count、body 放源文件名，不再重复 `Ready`/routes parsed 文案，Pre-request/Realtime/Runner/Tests panel header 也会隐藏 `idle`、`Runner idle`、`No requests`、`No results` 和零测试计数，并将 Pre-request action/error、Tests configured、Runner running 和 Realtime error meta 压短为 `act`/`Err`/`cfg`/`Run`，panel header meta 改为内容自适应且最多占 180px，空白 meta 不再预留右侧槽，减少常态文字噪音和窄 pane 标题挤压；Request editor tabs 已支持 Ctrl/Cmd+1..7 直接切换并复用鼠标 tab 的滚动复位和临时层关闭逻辑；Response viewer tabs 已支持 Ctrl/Cmd+Shift+1..3 直接切换并复用鼠标 tab 的滚动复位和临时层关闭逻辑；URL/Send、Open/Import/Export/Save、History Clear/Del、Vars、WS/SSE、Headers Copy、Response Copy/Fold/Open、Codegen Copy/Language、Runner Run/Stop Fail 和 Collection menu action 均复用当前状态前置条件；更多前置条件不满足时动作禁用和快捷键覆盖仍需继续系统复核
  - [x] Busy 期间 Route 选择、History 恢复和 Collection request 恢复已禁用并阻止入口，避免左栏替换正在运行的 Request 配置；Collection `+ Req`/`+ Dir`/Copy/Del/Rename/drag move 也会在 busy 期间阻止集合结构变更，拖拽预览与 drop-over 反馈同样禁用。
  - [x] Busy 期间 Import path、Saved JSON path 和 Collection rename 输入会进入真实 disabled 状态；Route/History filter 保持可编辑，Ctrl/Cmd+F 会跳过 disabled 的 Saved JSON path 目标。
  - [x] Busy 期间 GraphQL Templates 的 Use 操作和 Tests Clear 已禁用并补 Rust-side guard，避免运行中改写 GraphQL 输入或清空测试输出。
  - [x] Collection Import/Export/Save 函数入口已补 busy guard；Response Copy 按钮和入口已共用同一 active-view/非空内容/idle 前置条件。
  - [x] Ctrl/Cmd+Enter 已绑定为从当前 Request editor 上下文发送请求，并复用 Send 按钮的 busy、URL 和 pre-request URL 前置条件。
  - [x] History Clear 与单行 Del 已改为 idle-state 前置条件，按钮禁用态和回调入口都会复查当前 busy 状态；History filter 保持可编辑。
  - [x] WS Open/Send/End、SSE Once/Stream/Stop 以及 Realtime Copy/Clear 已改为 busy-aware 前置条件，渲染态和函数入口复用同一 idle-state 规则。
  - [x] 顶栏 Import popover toggle 与 Headers Copy 已补 busy-aware 前置条件，避免只依赖通用按钮 helper 的渲染时 enabled。
  - [x] Codegen Copy 已改为点击时重新构建当前请求 snippet 并复查 busy/URL/build 前置条件，避免复制渲染时捕获的过期代码片段。
  - [x] Raw Body Format 入口已复用渲染态前置条件，只在 idle、JSON raw 模式且 body 非空时执行格式化。
  - [x] Codegen language selector 与 Response Fold/Open 已补 callback 当前状态复查，避免 busy 或响应内容变化后仍执行渲染时捕获的 enabled 动作。
  - [x] Request Send 按钮点击、URL 回车和发送快捷键已统一复用当前 app 状态的 busy/URL/pre-request URL 前置条件；Response Copy 点击也改为直接走入口 guard，不再依赖渲染时捕获的 enabled。
  - [x] `action_button`、`top_bar_action_button`、`sidebar_action_button`、`sidebar_fluid_button` 和 `panel_action_button` 共享 helper 已在点击时复查当前 busy 状态，避免 render 时 enabled 但异步进入 busy 后仍执行回调。
  - [x] Request method selector 和 method menu item 已统一复用当前 idle-state 前置条件，避免 busy 状态下通过残留菜单项改写请求方法。
  - [x] Runner Stop Fail toggle 已在点击时复查当前 runner/busy 状态，不再依赖渲染时捕获的 enabled。
  - [x] Collection context menu 入口已复用集合 mutation busy guard，busy 期间右键菜单不会打开，避免只依赖菜单内按钮禁用。
  - [x] Params/Headers/Vars/Form/URL Enc/WebSocket/SSE key/value 编辑器已提供统一 `+`/`x` 行增删入口，并在 busy、无 active env 或 index 越界时禁用且回调入口复查当前状态。
  - [x] Tests assertion 表头已提供 `+` 新增入口，行内已提供 `x` 删除入口，kind selector 和行操作回调都会复查 busy 与 index，避免运行中改写测试配置。
  - [x] Headers 面板已移除内联说明句，bulk/preset 用法保留在文档中，Request pane、URL 缺失校验与 bulk paste 失败响应内不再额外显示教程文字。
  - [x] Text/Hex mode 已移除内联输入提示句，消息输入 placeholder 收敛为 `Message`，Realtime log 动作使用短标签 `Copy`/`Clear`，空/复制/清空反馈标题统一为 `No log` / `Copied` / `Cleared`，成功 copy/clear 正文统一为 `Log.`，不再重复 `WebSocket`/`SSE` 协议名或动作词。
  - [x] Realtime panel header meta 已从 worker 内部状态收敛为 `Conn` / `Open` / `TX` / `RX text` / `Evt ...` / `1 ev` / `Closed` / `No URL` / `Bad URL` 等短标签，SSE retry meta 也从 `retry ...` 收敛为 `r...`。
  - [x] WebSocket/SSE URL 校验响应已从 `Enter ... before ...` 收敛为短事实反馈，空 URL 与 scheme 不匹配分别显示 `URL is empty.` / `Expected WS(S).` / `Expected HTTP(S).`，标题也收敛为 `WS no URL` / `SSE no URL` / `Bad WS URL` / `Bad SSE URL`，Realtime panel header meta 同步压缩为 `No URL` / `Bad URL`。
  - [x] WebSocket Send、Hex binary parse 和 Raw JSON Format 的失败反馈已收敛为 `No active session.`、`Hex body is empty.`、`JSON body is empty.` 等短事实文案，WS/SSE active/message/error/subscribe 等 Response 标题也统一使用 `WS active` / `WS msg` / `WS failed` / `SSE sub` / `SSE open` 这类短缩写，不再在 Response pane 展示示例、操作步骤、完整协议名或较长状态动词。
  - [x] Import/Collection path、空集合导出、Runner 空请求、Mock 无路由和 Env name 校验已统一为 `No path` / `No export` / `No requests` / `No routes` 短标题，以及 `Path is empty.`、`No saved requests.`、`No routes loaded.`、`Env name is empty.` 等短事实正文，Mock 无路由状态栏提示也收敛为 `No routes`。
  - [x] File/editor/request transport/mock/realtime/runner worker 错误 helper 已移除后续说明块，Response pane 保留对象上下文和 Error 段，worker fallback 收敛为 `Request worker stopped.` / `Collection runner stopped.`。
  - [x] Collection import/export/save、Runner active、Env active/create/delete、Headers/Realtime copy/clear、SSE stop 和 Raw JSON Format 成功/忙碌反馈标题与正文已收敛为 `Imported` / `Exported` / `Saved`、对象名、路径、URL 或 `Copied` / `Cleared` / `Running` / `Formatted` 等短状态词；Runner active 反馈从 `Runner active` + `Running.` 改为 `Running` + `Runner.`，SSE stop 反馈从 `SSE stop` + `Stopped.` 改为 `Stopped` + `SSE.`；Env active/create/delete 反馈从 `Env active` + `Active.` 等重复组合改为 `Active` / `Created` / `Removed` + env name + `Env.`；Raw JSON Format 成功反馈从 `Body formatted` + `Formatted.` 改为 `Formatted` + `JSON` + `Body.`；Headers bulk copy 成功反馈也从 `Headers copied` + `Copied.` 改为 `Copied` + count + `Headers.`，空 headers 复制反馈改为 `No copy` + `No headers.`。
  - [x] Request Save、Collection Restore、Headers bulk paste 和 WS closed fallback 已继续收敛为短事实反馈，减少 Response pane 内完整句子挤压；Request Save 从 `Saved` + collection name + `Saved.` 改为 `Saved` + collection name + `Request.`，Collection request restore 从 `Collection request` + `Restored.` 改为 `Restored` + request name + `Request.`；Headers bulk paste 成功反馈也从 `Headers pasted` + `Applied.` 改为 `Applied` + count + `Headers.`，空剪贴板/无可解析 header 收敛为 `No paste` + `Clipboard empty.` / `No headers.`；Header preset 反馈从 `Header preset applied` + `name: value` 收敛为 `Applied` + header name + `Header.`，避免把 header 值塞进 Response pane。
  - [x] Response 错误标题继续收敛为 `Build fail` / `Bad tests` / `Request fail` / `Save fail` / `Format fail` / `Run fail` 等短状态词，完整上下文保留在正文首行和 Error 段，避免 header/status 区重复长句。
  - [x] File/editor/request transport/mock/realtime 错误正文首行已从长句收敛为 `Request failed.`、`Mock start failed.`、`WS session failed.`、`SSE fetch failed.` 等短 action line，具体对象和原始错误保留在后续字段中。
  - [x] Headers bulk 空剪贴板/空解析、WebSocket Hex 奇数长度/非法字节、Tests assertion 空字段/非法状态范围、SSE retry 状态、GraphQL schema summary/browser、Runner 汇总统计/pre-request action/header 行以及 WebSocket/SSE 日志输出已继续收敛为短事实反馈/短指标行，避免 Response pane、Tests 行、Realtime meta 和 GraphQL 面板出现完整说明句。

### 13.2 功能完整性审查
- [~] 13.2.1 请求构建器完备度审查: Headers 已支持 key/value 编辑、显式行增删、bulk copy/paste、解析常见 `-H`/`--header` 格式，以及短标签 `Accept` / `Content` / `Bearer` 常用预设且会按 header 名 upsert；Params、Vars、Form、URL Enc、WS Hdrs 和 SSE Hdrs 也已复用同一行增删入口；Body 已覆盖 none/form-data/urlencoded/raw/GraphQL/binary，Raw JSON 增加局部 Format 操作和 `Preview` 结构预览，GraphQL `Payload`/`Schema`/`Fields`/`Templates` 已改为短标题、固定高度边界、18px 正文和内部垂直滚动，Templates 的应用入口与变量预览已收敛为 `Use`/`Vars` 短标签，Schema summary/browser 已改为 `Roots Q=...`、`Types ...`、`Fields ...`、`Dirs ...`、`Q/M/S`、`Obj/In/Enum/Scalar` 短指标行，避免长文本预览挤乱 Request pane 或被裁掉；Raw body、GraphQL query 和 GraphQL vars 已接入 multiline 输入基础能力，支持粘贴保留换行、CRLF 规范化、Enter/Shift+Enter 插入换行、body 级空状态 placeholder 和内部滚动；Auth 已覆盖 None/Bearer/OAuth 手动 access token/Basic/JWT/API header/query；OAuth 授权码/PKCE、redirect、refresh、Body 编辑器截图级复核仍待完成
- [~] 13.2.2 响应查看器完备度审查: `Pretty`/`Raw`/`Hdrs` 各模式已作为局部 tabs 保持一等入口，Response pane 标题固定为 `Response`，非 idle 状态与 Tests 摘要合并到右侧 meta，Response tab 和 Fold/Open/Copy 固定按钮宽度已收紧以减少右 pane 工具条文字挤压；响应文本使用只读可选择 viewer，Ctrl/Cmd+A 与 Ctrl/Cmd+C 已绑定，viewer 根节点也已补 min-width/overflow/wrap 边界避免长行撑开 pane，且 Response body 内层不再强制 `size_full` 高度，让长响应文本自然高度参与外层滚动；Response body、Tests/Pre-request/Runner/Mock/Realtime 等局部结果与日志正文已提升到 18px，WebSocket/SSE 输出已改为 `URL`/`TX`/`RX`/`#id`/`r...` 短标签；普通响应 header meta 已收敛为 Tests 摘要，无 Tests 时保持空白，减少右侧 header 文字噪音；Response tab 行已增加局部 Copy 操作用于复制当前 Pretty/Raw/Hdrs 视图内容，且不污染全局顶栏；Copy 会拒绝空当前视图和空 Hdrs 占位文案，Hdrs 视图对空白 headers 显示 `No headers` 短空态；Fold/Open 会拒绝非 JSON 内容；长响应内容的交互复核和截图级复核仍待完成
- [~] 13.2.3 核心交互流畅度审查: 请求发送中 Response body 已显示 `Pending` + method + URL 短占位，不再保留旧响应正文或重复 Response 区域名；请求成功、请求错误和后台 worker 取消路径都会恢复 Busy 状态并写入 Response/History；路由选择、普通响应、错误响应、Pretty/Raw/Hdrs 切换和 Pretty Fold/Open 现在都会将 Response body 滚动位置复位到顶部，避免长响应残留旧滚动偏移；路由选择→请求发送→响应展示的截图级交互复核仍待完成
### 13.3 稳定性与错误处理
- [~] 13.3.1 复杂规格文件导入交互审查: 多路由、多分组场景下过滤、选中、滚动和空状态保持可用；Collection tree 深层目录缩进已加上上限并有 metric 回归测试，避免多层导入内容把左 pane 可读区域挤空；OpenAPI 多路由截图级复核仍待完成
- [~] 13.3.2 长响应内容交互审查: 复杂 JSON/长文本响应的选择、复制、折叠和滚动行为保持稳定；只读响应 viewer 已补自身 min-width/overflow/wrap 边界，并移除 Response body 内层强制 full-size 高度，避免长行内容反向撑开右 pane或长文本被固定高度裁掉；截图级复核仍待完成
- [~] 13.3.3 错误处理完善: OpenAPI 导入、Collection 导入/导出、请求构建、请求发送、测试配置、WebSocket、SSE、Collection Runner 和 Mock 服务失败已输出包含操作上下文、路径/URL/端口与底层错误的 Response pane 文案；说明式后续步骤已从应用内反馈移除，常见错误首行和校验正文保持短事实反馈；Collection/Mock/realtime 状态行也会同步失败状态；仍需截图级复核和少量边缘状态审查

### 13.4 文档体系
- [x] 13.4.1 编写用户使用指南 `docs/07_USER_GUIDE.md`: 安装、导入规格、发送请求、启动 Mock、变量/环境、集合管理、Runner 与 CLI 均已覆盖
- [x] 13.4.2 编写开发者文档 `docs/06_DEV_GUIDE.md`: 项目架构、模块职责、构建流程、贡献指南、验证基线均已覆盖
- [x] 13.4.3 维护 `docs/02_DESIGN.md` 至最新设计决策状态：已补充 Runner/CLI 与文档边界决策
- [x] 13.4.4 维护 `docs/01_PRD.md` 至最新产品方向，已更新当前本地工作站能力与 Backlog
- [x] 13.4.5 维护 `docs/TODO.md`（本文档）的任务状态与附录准确

### 13.5 跨平台验证与发布
- [ ] 13.5.1 Linux (Wayland/X11) 构建与功能验证
- [ ] 13.5.2 Windows 构建与功能验证
- [ ] 13.5.3 macOS 构建与功能验证
- [ ] 13.5.4 发布 v0.1.0

---

## 附录 A: 文件产出清单

| 文件 | 说明 | 阶段 |
|------|------|------|
| `docs/01_PRD.md` | 产品需求与 MVP 范围 | 0.1 |
| `docs/02_DESIGN.md` | 设计规范（颜色/字体/间距/组件/交互） | 0.1 |
| `docs/03_LAYOUT.md` | 页面布局规格（8 个页面的面板分割/响应式规则） | 0.3 |
| `docs/04_COMPONENTS.md` | Slint 组件设计规范（12 个组件的视觉规格/状态定义） | 0.3 |
| `docs/05_TODO.md` | 本文档：开发路线图与任务追踪 | 0 |
| `docs/06_DEV_GUIDE.md` | 开发者指南（项目架构/模块职责/构建流程） | 13.4 |
| `docs/07_USER_GUIDE.md` | 用户使用指南 | 13.4 |
| `docs/08_SCRIPTING.md` | 脚本与测试引擎评估 | 10.1 |
| `docs/09_GRPC.md` | gRPC 支持评估与实现拆分 | 11.4 |

## 附录 B: 当前状态概览

| 模块 | 状态 | 技术栈 |
|------|------|--------|
| 应用外壳 | GPUI 应用壳已替换 Slint 原型 | GPUI + gpui_platform |
| OpenAPI 解析 | 已实现 | serde_json, serde_yaml |
| HTTP 客户端 | 核心已实现，UI 细节待完善 | reqwest |
| Mock 服务器 | 已实现 | Axum, permissive CORS |
| UI 布局 | MVP 工作台布局已接入 | GPUI |
| 环境变量 | 核心与动态环境管理 UI 已实现，视觉对齐待检查 | — |
| 集合系统 | 数据模型、Postman 导入导出、侧栏树、右键菜单、拖拽移动和保存当前请求已实现；Bru 格式实现待补齐 | serde_json |
| 请求历史 | 数据模型、自动记录、侧边栏过滤/恢复/删除/清空与可见行测试已覆盖 | — |
| 代码生成 | cURL/Python/JavaScript/Rust/Go 已实现，cURL 本地执行验证已覆盖 | — |
| 集合运行器 | 顺序执行核心、GPUI 入口与 `zenapi run` CLI 已实现；并行调度待后续增强 | reqwest |
| 脚本/测试 | Pre-request script-lite 与原生响应断言已实现；完整脚本引擎与 `pm.*` 兼容待后续评估 | — |
| 多协议 | 远期 | — |
| 插件系统 | 远期 | — |
