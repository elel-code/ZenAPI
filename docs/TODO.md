# ZenAPI 开发 TODO

> 最后更新: 2026-06-13
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
> | **Hoppscotch** | 开源轻量，Web/PWA 优先 | Vue.js + Nuxt + TS | MIT | 轻量理念、多协议 |
> | **Bruno** | Git-native，本地文件存储 | Electron + JS | MIT | 离线优先、Bru 标记语言 |
> | **Insomnia** | Kong 维护，设计+调试一体 | Electron + React | Apache 2.0 | OpenAPI 原生、深色主题 |
> | **Yaak** | 隐私优先，Rust 原生桌面 | Tauri + Rust + React | 专有(源码可用) | 最近技术参照 |
>
> **设计参考**: 综合 Postman 的功能深度、Hoppscotch 的轻量理念、Bruno 的文件存储和 Yaak 的原生桌面体验——**目标为功能完备且启动 <1s 的原生 API 工作站**。
>
> **当前状态**: 已补充离线基线文档，用于固化 GPUI 重构、Linux `gpui_platform`、依赖升级策略和无 bundled font 资产等方向；实时联网截图、版本号和精确测量仍待复核。

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
- [~] 0.1.11 输出 UI 视觉调研文档: `docs/RESEARCH_UI.md` 离线基线已输出，实时截图/测量待复核

### 0.2 交互模式调研 【UI 核心·最高优先】
- [ ] 0.2.1 侧边栏交互横评: 集合树展开/折叠、拖拽排序、右键菜单、搜索过滤、收藏
- [ ] 0.2.2 请求构建器交互横评: 方法选择器、URL 输入、Params/Headers/Body 编辑区切换
- [ ] 0.2.3 标签页系统横评: Postman 的多标签 vs Hoppscotch 的单页 vs Yaak 的原生标签
- [ ] 0.2.4 双栏/单栏视图: Postman 的 Two Pane View、各客户端响应区布局策略
- [ ] 0.2.5 键盘快捷键体系横评
- [ ] 0.2.6 PWA/离线体验: Hoppscotch 的 Service Worker 策略启发 ZenAPI 原生离线
- [~] 0.2.7 输出交互调研文档: `docs/RESEARCH_INTERACTION.md` 离线基线已输出，实时行为复核待补齐

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
- [~] 0.3.17 输出功能对比文档: `docs/RESEARCH_FEATURES.md` 离线基线已输出，实时功能覆盖复核待补齐

### 0.4 技术架构调研
- [ ] 0.4.1 横评技术栈: Electron(Postman/Bruno/Insomnia) vs Vue+PWA(Hoppscotch) vs Tauri+Rust(Yaak) vs GPUI+Rust(ZenAPI)
- [~] 0.4.2 横评应用体积: ZenAPI release 37M / stripped 28M 已测；Postman/Hoppscotch/Yaak 体积与 10MB 优化目标待联网复核
- [ ] 0.4.3 横评启动时间、内存占用、离线能力
- [~] 0.4.4 输出技术架构调研文档: `docs/RESEARCH_ARCH.md` 离线基线已输出，体积/启动/内存实测待补齐

### 0.5 调研总结与优先级调整
- [~] 0.5.1 汇总调研发现，明确 ZenAPI 的优势方向: 原生 Mock Server、GPUI 原生性能、OpenAPI 深度集成（离线基线已写入）
- [~] 0.5.2 识别当前功能缺口: 多协议(WebSocket/gRPC)、插件系统、AI 辅助（离线基线已写入）
- [ ] 0.5.3 基于调研结果调整本文档后续阶段的优先级排序
- [~] 0.5.4 输出调研总结: `docs/BENCHMARK.md` 离线基线已输出，实测 benchmark 待补齐

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
- [x] 3.4 实现 Query Params 编辑面板: 键值对表格、自动拼接到 URL
- [x] 3.5 实现 Request Body 编辑面板:
  - [x] 3.5.1 支持 `none` / `form-data` / `x-www-form-urlencoded` / `raw` / `binary` 类型切换
  - [x] 3.5.2 `raw` 模式下支持 JSON/XML/Text/HTML 内容类型切换，并提供 JSON/XML/HTML 轻量语法高亮预览（`syntect` 保留为未来增强选项）
  - [x] 3.5.3 `form-data` 支持文本字段和 `@file` 文件附件
- [~] 3.6 实现 Authorization 面板:
  - [x] 3.6.1 Bearer Token
  - [x] 3.6.2 Basic Auth
  - [x] 3.6.3 API Key (Header/Query)
  - [ ] 3.6.4 OAuth 2.0（远期）
  - [x] 3.6.5 JWT
- [x] 3.7 实现发送请求: 通过 `reqwest` 发送，显示加载状态，按钮禁用防重入
- [x] 3.8 实现响应查看器:
  - [x] 3.8.1 状态码 + 耗时 + 响应大小元数据显示（260px 右对齐，14px 右内边距）
  - [x] 3.8.2 响应体 Pretty 模式: JSON 格式化、缩进与折叠摘要均已支持
  - [x] 3.8.3 响应体 Raw 模式: 原始文本
  - [x] 3.8.4 Response Headers 子标签页
  - [x] 3.8.5 响应查看器使用平台 monospace 样式，并通过专用只读可选中文本组件避免编辑光标
- [x] 3.9 验证: 本地 Axum HTTP 请求测试覆盖状态码、响应大小、响应头、Raw body 与 Pretty body
- [~] 3.10 UI 对齐检查: Headers/Body/Response 面板间距一致性、控件颜色使用共享 UI 辅助函数（非硬编码）、HTTP 方法色映射统一；已将 HTTP 方法标签宽度、平台 monospace、主工作区 surface/border/text/accent 色和核心列表 metric 收敛到共享常量/辅助函数，视觉参照 `docs/RESEARCH_UI.md` 调研横评仍需截图复核

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
- [~] 5.7 UI 对齐检查: 变量编辑器键值表格列宽对齐、行高一致、环境选择器下拉与全局变量面板控件风格统一、删除按钮/新增行交互反馈一致；键值表格主列宽已抽成共享 UI metric 并增加回归测试，视觉参照 `docs/RESEARCH_UI.md` 调研横评仍需截图复核

---

## 阶段 6: 集合系统

> **目标**: 实现集合系统，支持请求的组织、存储和复用。
> 集合数据以纯文本 JSON 存储在本地文件系统，天然 Git 友好。

- [x] 6.1 设计集合数据模型: 集合 → 文件夹 → 请求的三级结构
- [x] 6.2 实现集合序列化/反序列化 (JSON 格式，兼容 Postman Collection v2.1 格式)
- [x] 6.3 探索 Bruno 的 Bru 标记语言作为可选的集合存储格式：输出 `docs/BRU_FORMAT.md`，暂定先保留原生 JSON，后续优先做 Bru-style export
- [x] 6.4 实现集合侧边栏:
  - [x] 6.4.1 集合树渲染: 展开/折叠
  - [x] 6.4.2 右键菜单: 新建请求/文件夹、重命名、删除、复制
  - [x] 6.4.3 请求拖拽移动
- [x] 6.5 实现请求保存: 从当前请求构建器保存到集合
- [x] 6.6 实现集合导入/导出: 兼容 Postman Collection JSON 格式
- [x] 6.7 验证: Postman Collection JSON 模型导入/导出与集合树可见行层级均有自动化测试覆盖
- [~] 6.8 UI 对齐检查: 集合树节点间距/缩进一致、展开折叠图标尺寸固定、右键菜单样式与系统其他菜单一致、拖拽排序视觉反馈平滑、空文件夹/空集合占位与侧边栏空状态风格统一；集合树行高/缩进/图标宽度/拖拽色已抽成共享 UI metric 并增加回归测试，交互参照 `docs/RESEARCH_INTERACTION.md` 调研横评仍需截图复核

---

## 阶段 7: 请求历史

> **目标**: 自动保存请求历史，支持回溯和复用。

- [x] 7.1 设计历史记录数据模型: 时间戳、请求详情、响应摘要
- [x] 7.2 实现请求自动记录: 每次发送请求后自动保存
- [x] 7.3 实现历史侧边栏视图: 按时间倒序、搜索过滤
- [x] 7.4 实现历史条目操作: 点击恢复请求、删除单条/批量清除
- [x] 7.5 验证: 自动记录、过滤/恢复模型、历史侧边栏可见行限制与无匹配空状态均有自动化测试覆盖

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
- [x] 9.2 实现运行器 UI: 当前集合 Run All、遇错停止切换、执行状态与结果摘要已接入 GPUI
- [x] 9.3 实现 CLI 运行器: `zenapi run collection.json` 命令行入口，支持 `--delay-ms` 与 `--stop-on-failure`
- [x] 9.4 验证: 本地 Axum 集合运行测试覆盖成功/失败汇总、继续执行与遇错停止；CLI 帮助路径已验证

---

## 阶段 10: 脚本与测试（远期）

> **目标**: 支持 Pre-request Scripts 和 Tests 脚本，实现请求前置处理和响应断言。

- [x] 10.1 评估 Rust 嵌入脚本引擎方案: 已输出 `docs/SCRIPTING.md`，建议先定义宿主 API/结果模型，优先 Rhai，小心评估 `deno_core` 体积成本
- [~] 10.2 实现 Pre-request Script 编辑器和执行: 已实现原生 script-lite action line，支持 method/url/header/query/body/变量覆盖，单次发送、代码生成、GPUI Runner 与 CLI Runner 共用执行路径；保存集合时保留 raw 请求字段和 action line，执行时再应用；完整脚本引擎、多行编辑器和执行日志待实现
- [~] 10.3 实现 Tests 脚本编辑器和断言 API: 已实现原生 Tests 编辑器与 Rust 响应断言模型（状态码、Header、Body、JSON Path、耗时、大小），支持保存/恢复到集合请求；`pm.test` / `pm.expect` 脚本兼容层待实现
- [~] 10.4 实现测试结果展示面板: 单次请求 Tests 面板、Runner 结果行与响应摘要均已展示断言通过/失败详情；Pre-request 面板已显示最近 action 执行数量或构建错误；GPUI Runner 摘要与 CLI 输出已记录 pre-request action 名称/目标；完整脚本引擎运行日志待实现
- [~] 10.5 验证: 响应断言编辑器字段解析、断言成功/失败、预期 500、JSON Path 失败、runner 汇总和 pre-request script-lite 执行均有自动化测试覆盖；完整脚本引擎执行验证待实现

---

## 阶段 11: 多协议支持（远期）

> **目标**: 扩展协议支持，覆盖 GraphQL、WebSocket、SSE 和 gRPC。

- [ ] 11.1 GraphQL 支持: 内省查询、Schema 浏览、查询编辑器
- [ ] 11.2 WebSocket 支持: 连接管理、消息发送/接收面板
- [ ] 11.3 SSE (Server-Sent Events) 支持
- [ ] 11.4 gRPC 支持评估: 依赖 protobuf 编译，标记为探索性任务

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

> **目标**: UI 细节对齐与调研参照、稳定性和性能优化、文档完备，准备首次发布。
>
> **UI 持续优化原则**: 本阶段是对前序各阶段 UI 对齐检查（3.10/5.7/6.8）的汇总校验。
> 每个功能模块应在实现阶段当即完成 UI 对齐，不把 UI 债堆积到最后一次性修。

### 13.1 UI 系统完整性审查（参照: `docs/RESEARCH_UI.md` + `docs/RESEARCH_INTERACTION.md`）
- [ ] 13.1.1 颜色审查: 全局表面色/边框色/强调色一致、HTTP 方法色映射（如 1.2/2.3 定义）无硬编码色值、响应状态色调（绿/红/琥珀/灰）全程统一
- [ ] 13.1.2 字体排版审查: 正文/代码字体一致、字号层级不乱跳、monospace 覆盖 URL/文件路径/API 路径/代码体/本地服务地址、placeholder 字体族与输入值一致
- [ ] 13.1.3 间距与尺寸审查: 标签页行 40px 固定高度 + 2px 活跃下划线；请求地址栏 36px 在 52px 工具带内显式 y 偏移；响应状态元数据 260px 右对齐 14px 右内边距；侧边栏选中路由 3px 主色左边标记；导入弹出窗口 520×58px
- [ ] 13.1.4 控件样式审查: 按钮主色/次要/危险/禁用状态由共享 UI 辅助函数统一管理；禁用控件无指针手势；侧边栏 HTTP 方法为定宽文本非填充徽章；弹出窗口对齐触发控件原点
- [ ] 13.1.5 交互规范审查: 前置条件不满足时动作禁用且 UI 可见表达；导入新规格自动停止运行中的 Mock 服务；过滤仅改变视图不改变全局操作计数；单行输入 Enter 触发主动作；不预埋未实现功能的标签或控件

### 13.2 功能完整性审查（参照: `docs/RESEARCH_FEATURES.md`）
- [ ] 13.2.1 请求构建器完备度审查: Headers/Body/Auth 编辑器功能覆盖
- [ ] 13.2.2 响应查看器完备度审查: Pretty/Raw/Headers 各模式功能完整
- [ ] 13.2.3 核心交互流畅度审查: 路由选择→请求发送→响应展示的往返延迟
- [ ] 13.2.4 启动性能与内存占用审查
- [ ] 13.2.5 更新 `docs/BENCHMARK.md` 调研总结至最新状态

### 13.3 性能与稳定性
- [ ] 13.3.1 大规格文件导入流畅度优化（目标：1000+ 路由的 OpenAPI 文件 <2s 解析渲染）
- [ ] 13.3.2 响应体渲染性能优化（大 JSON 响应的增量渲染和滚动性能）
- [ ] 13.3.3 启动时间优化（目标 <1s）
- [ ] 13.3.4 错误处理完善: 网络错误、文件解析错误、Mock 启动失败的用户友好提示
- [ ] 13.3.5 可执行文件大小优化（目标 ~10 MB）

### 13.4 文档体系
- [x] 13.4.1 编写用户使用指南 `docs/USER_GUIDE.md`: 安装、导入规格、发送请求、启动 Mock、变量/环境、集合管理、Runner 与 CLI 均已覆盖
- [x] 13.4.2 编写开发者文档 `docs/DEV_GUIDE.md`: 项目架构、模块职责、构建流程、贡献指南、验证基线均已覆盖
- [x] 13.4.3 维护 `docs/DESIGN.md` 至最新设计决策状态：已补充 Runner/CLI 与文档边界决策
- [x] 13.4.4 维护 `docs/PRD.md` 至最新产品方向，已更新当前本地工作站能力与 Backlog
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
| `docs/TODO.md` | 本文档 | 0 |
| `docs/RESEARCH_UI.md` | 五方 UI 视觉横评 | 0.1 |
| `docs/RESEARCH_INTERACTION.md` | 五方交互模式横评 | 0.2 |
| `docs/RESEARCH_FEATURES.md` | 六方功能对比矩阵 | 0.3 |
| `docs/RESEARCH_ARCH.md` | 技术架构横评 | 0.4 |
| `docs/BENCHMARK.md` | 调研总结 | 0.5 |
| `docs/BRU_FORMAT.md` | Bru-style 集合存储探索记录 | 6.3 |
| `docs/SCRIPTING.md` | 脚本与测试引擎评估 | 10.1 |
| `docs/USER_GUIDE.md` | 用户使用指南 | 13.4 |
| `docs/DEV_GUIDE.md` | 开发者文档 | 13.4 |

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
