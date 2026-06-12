# ZenAPI 开发 TODO

> 最后更新: 2026-06-13
> 本文档是 ZenAPI 的开发路线图和任务清单，按阶段组织。
> 已完成的项标记为 `[x]`，进行中的标记为 `[~]`，待完成的标记为 `[ ]`。

---

## 阶段 0: 竞品实时调研与功能对标 【重点·优先】

> **目标**: 通过实时联网调研 5 个主流 API 客户端的 UI 样式、交互模式和功能集，
> 形成对标分析文档，锁定 ZenAPI 的差异化定位。
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
> **ZenAPI 设计定位**: 取 Postman 的功能深度，取 Hoppscotch 的轻量理念，用 Rust/GPUI 实现 Bruno 的 Git-native 文件存储和 Yaak 的原生桌面体验——**功能完备且启动 <1s 的原生 API 工作站**。

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
- [ ] 0.1.11 输出 UI 视觉调研文档: `docs/RESEARCH_UI.md`

### 0.2 交互模式调研 【UI 核心·最高优先】
- [ ] 0.2.1 侧边栏交互横评: 集合树展开/折叠、拖拽排序、右键菜单、搜索过滤、收藏
- [ ] 0.2.2 请求构建器交互横评: 方法选择器、URL 输入、Params/Headers/Body 编辑区切换
- [ ] 0.2.3 标签页系统横评: Postman 的多标签 vs Hoppscotch 的单页 vs Yaak 的原生标签
- [ ] 0.2.4 双栏/单栏视图: Postman 的 Two Pane View、各客户端响应区布局策略
- [ ] 0.2.5 键盘快捷键体系横评
- [ ] 0.2.6 PWA/离线体验: Hoppscotch 的 Service Worker 策略启发 ZenAPI 原生离线
- [ ] 0.2.7 输出交互调研文档: `docs/RESEARCH_INTERACTION.md`

### 0.3 核心功能对比矩阵
- [ ] 0.3.1 Collections 对比: 组织结构(集合→文件夹→请求)、导入导出、Postman Collection v2.1 兼容性
- [ ] 0.3.2 Environments & Variables 对比: 作用域(全局/集合/环境)、`{{var}}` 语法、Bruno 的 Bru 变量
- [ ] 0.3.3 Pre-request Scripts 对比: 执行时机、可用 API、Hoppscotch 的简化脚本
- [ ] 0.3.4 Tests 对比: 断言语法、测试结果展示、Bruno 的 CLI 运行器
- [ ] 0.3.5 Authorization 对比: 认证类型覆盖度(Bearer/Basic/OAuth2/API Key/Digest/JWT)
- [ ] 0.3.6 Request Body 对比: form-data/x-www-form-urlencoded/raw/binary/GraphQL 编辑
- [ ] 0.3.7 Headers & Params 对比: 批量编辑、自动补全、Insomnia 的 Header 预设
- [ ] 0.3.8 Mock Server 对比: Postman Mock vs Insomnia Mock vs ZenAPI 本地 Mock 差异化
- [ ] 0.3.9 Code Generation 对比: 语言覆盖度(所有客户端均支持 cURL + 多语言)
- [ ] 0.3.10 History 对比: 历史保存策略、搜索、Yaak 的本地加密存储
- [ ] 0.3.11 多协议支持对比: REST / GraphQL / WebSocket / gRPC / SSE / MQTT / Socket.IO
- [ ] 0.3.12 自托管/离线能力对比: Hoppscotch Docker 自托管、Bruno 本地文件系统、Yaak 完全本地
- [ ] 0.3.13 AI 辅助能力对比: Postbot / Hoppscotch AI / Insomnia AI / ZenAPI 差异化机会
- [ ] 0.3.14 调研 Bruno 的 Bru 纯文本标记语言: 语法设计、Git 友好性、人类可读性
- [ ] 0.3.15 调研 Yaak 的插件系统: 认证插件、模板标签、UI 定制扩展点
- [ ] 0.3.16 调研 Insomnia 的 OpenAPI 编辑器: 可视化预览、设计模式
- [ ] 0.3.17 输出功能对比文档: `docs/RESEARCH_FEATURES.md`

### 0.4 技术架构调研
- [ ] 0.4.1 横评技术栈: Electron(Postman/Bruno/Insomnia) vs Vue+PWA(Hoppscotch) vs Tauri+Rust(Yaak) vs GPUI+Rust(ZenAPI)
- [ ] 0.4.2 横评应用体积: Postman ~400MB vs Hoppscotch <10MB(PWA) vs Yaak ~15MB(Tauri) vs ZenAPI 目标 ~10MB
- [ ] 0.4.3 横评启动时间、内存占用、离线能力
- [ ] 0.4.4 输出技术架构调研文档: `docs/RESEARCH_ARCH.md`

### 0.5 对标分析与差异化定位
- [ ] 0.5.1 输出 ZenAPI vs Postman vs Hoppscotch vs Bruno vs Insomnia vs Yaak 六方功能差异矩阵
- [ ] 0.5.2 标注 ZenAPI 的硬差异优势: 原生 Mock Server、GPUI 原生性能、OpenAPI 深度集成
- [ ] 0.5.3 标注 ZenAPI 的机会缺口: 多协议(WebSocket/gRPC)、插件系统、AI 辅助
- [ ] 0.5.4 基于对标结果调整本文档后续阶段的优先级排序
- [ ] 0.5.5 输出对标报告: `docs/BENCHMARK.md`

---

## 阶段 1: GPUI 应用外壳重写

> **目标**: 替换现有 Slint 原型代码，建立 GPUI 应用架构基础。
> 遵循 PRD 中的兼容性策略：GPUI 重写是破坏性替换，不保留 Slint UI 文件。
> 颜色、间距等 UI 基准由本阶段 1.2 主题系统定义，视觉对标以阶段 0 调研为补充。

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

> **目标**: 实现完整的 HTTP 请求发送和响应展示功能。
> 对标 Postman 请求构建器的完备度，保持 Hoppscotch 的操作流畅感。

- [x] 3.1 实现请求方法选择器: GET/POST/PUT/PATCH/DELETE/OPTIONS/HEAD，下拉或分段控件
- [x] 3.2 实现 URL 地址栏: 单行输入、Enter 发送、与路由选择联动
- [~] 3.3 实现请求 Headers 编辑面板: 固定键值对表格与常用 Header 提示已支持，批量编辑待补齐
- [x] 3.4 实现 Query Params 编辑面板: 键值对表格、自动拼接到 URL
- [~] 3.5 实现 Request Body 编辑面板:
  - [x] 3.5.1 支持 `none` / `form-data` / `x-www-form-urlencoded` / `raw` / `binary` 类型切换
  - [~] 3.5.2 `raw` 模式下支持 JSON/XML/Text/HTML 内容类型切换，语法高亮待后续实现（预留 `syntect` 集成点，参考 Insomnia 的编辑器体验）
  - [x] 3.5.3 `form-data` 支持文本字段和 `@file` 文件附件
- [~] 3.6 实现 Authorization 面板:
  - [x] 3.6.1 Bearer Token
  - [x] 3.6.2 Basic Auth
  - [x] 3.6.3 API Key (Header/Query)
  - [ ] 3.6.4 OAuth 2.0（远期，参考 Yaak 的插件化认证方案）
  - [ ] 3.6.5 JWT（远期）
- [x] 3.7 实现发送请求: 通过 `reqwest` 发送，显示加载状态，按钮禁用防重入
- [~] 3.8 实现响应查看器:
  - [x] 3.8.1 状态码 + 耗时 + 响应大小元数据显示（260px 右对齐，14px 右内边距）
  - [~] 3.8.2 响应体 Pretty 模式: JSON 格式化、缩进已支持，折叠待后续实现（参考 Insomnia 的响应折叠效果）
  - [x] 3.8.3 响应体 Raw 模式: 原始文本
  - [x] 3.8.4 Response Headers 子标签页
  - [ ] 3.8.5 响应编辑器使用平台 monospace 样式，只读模式（可选中文本，无编辑光标）
- [ ] 3.9 验证: 发送真实 HTTP 请求，验证响应展示正确
- [ ] 3.10 UI 对齐检查: Headers/Body/Response 面板间距一致性、控件颜色使用共享 UI 辅助函数（非硬编码）、HTTP 方法色映射统一；编辑器使用平台 monospace 样式、只读响应查看器可选中文本且无编辑光标（参照 3.8.5；视觉对标以 `docs/RESEARCH_UI.md` 竞品横评为准）

---

## 阶段 4: 本地 Mock 服务器

> **目标**: 将现有的 Mock 服务器模块接入 GPUI，增强可用性。
> ZenAPI 的 Mock 服务器是差异化壁垒——竞品中只有 Postman 和 Insomnia 有类似功能。

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

> **目标**: 实现环境配置和变量替换，对标 Postman Environments 和 Bruno 的 Bru 变量模式。

- [x] 5.1 设计变量数据模型: 变量名、当前值、初始值、作用域（全局/环境），参考 Bruno 的文件系统变量
- [x] 5.2 实现变量解析引擎: `{{variableName}}` 语法替换
- [~] 5.3 实现环境管理 UI:
  - [~] 5.3.1 环境列表与创建/删除：已支持固定环境选择与清空 key 删除变量，任意环境创建待补齐
  - [x] 5.3.2 环境变量编辑器（键值对表格）
  - [x] 5.3.3 当前活跃环境指示
- [x] 5.4 实现全局变量管理 UI
- [x] 5.5 变量集成到请求构建器: URL/Headers/Body 中均支持 `{{var}}` 替换
- [x] 5.6 验证: 创建环境变量，在请求 URL 中使用，确认替换正确
- [ ] 5.7 UI 对齐检查: 变量编辑器键值表格列宽对齐、行高一致、环境选择器下拉与全局变量面板控件风格统一、删除按钮/新增行交互反馈一致（视觉对标以 `docs/RESEARCH_UI.md` 竞品控件横评为准）

---

## 阶段 6: 集合系统

> **目标**: 对标 Postman Collections，借鉴 Bruno 的文件系统存储理念。
> 集合数据以纯文本 JSON 存储在本地文件系统，天然 Git 友好。

- [x] 6.1 设计集合数据模型: 集合 → 文件夹 → 请求的三级结构
- [x] 6.2 实现集合序列化/反序列化 (JSON 格式，兼容 Postman Collection v2.1 格式)
- [ ] 6.3 探索 Bruno 的 Bru 标记语言作为可选的集合存储格式
- [ ] 6.4 实现集合侧边栏:
  - [ ] 6.4.1 集合树渲染: 展开/折叠
  - [ ] 6.4.2 右键菜单: 新建请求/文件夹、重命名、删除、复制
  - [ ] 6.4.3 请求拖拽移动
- [ ] 6.5 实现请求保存: 从当前请求构建器保存到集合
- [ ] 6.6 实现集合导入/导出: 兼容 Postman Collection JSON 格式
- [ ] 6.7 验证: 导入 Postman Collection JSON，树结构正确渲染
- [ ] 6.8 UI 对齐检查: 集合树节点间距/缩进一致、展开折叠图标尺寸固定、右键菜单样式与系统其他菜单一致、拖拽排序视觉反馈平滑、空文件夹/空集合占位与侧边栏空状态风格统一（交互对标以 `docs/RESEARCH_INTERACTION.md` 竞品侧边栏横评为准）

---

## 阶段 7: 请求历史

> **目标**: 自动保存请求历史，支持回溯和复用。参考 Yaak 的本地加密存储方案。

- [x] 7.1 设计历史记录数据模型: 时间戳、请求详情、响应摘要
- [x] 7.2 实现请求自动记录: 每次发送请求后自动保存
- [x] 7.3 实现历史侧边栏视图: 按时间倒序、搜索过滤
- [x] 7.4 实现历史条目操作: 点击恢复请求、删除单条/批量清除
- [~] 7.5 验证: 自动记录与过滤/恢复模型测试已覆盖，历史列表可见性待人工 UI 验证

---

## 阶段 8: 代码生成

> **目标**: 对标 Postman Code Snippets，参考 Hoppscotch 和 Yaak 的代码生成覆盖度。

- [x] 8.1 设计代码生成器架构: 语言模板 + 请求数据填充
- [x] 8.2 实现首批语言支持: cURL / Python (requests) / JavaScript (fetch) / Rust (reqwest) / Go (net/http)
- [x] 8.3 实现代码片段 UI: 语言选择下拉、代码预览、一键复制
- [x] 8.4 验证: 构建请求，生成 cURL 命令，在终端执行验证

---

## 阶段 9: 集合运行器（远期）

> **目标**: 对标 Postman Collection Runner 和 Bruno CLI，批量执行集合请求。

- [ ] 9.1 设计运行器调度模型: 顺序/并行执行、延迟控制、失败策略
- [ ] 9.2 实现运行器 UI: 集合选择、执行进度、结果摘要
- [ ] 9.3 实现 CLI 运行器: `zenapi run collection.json` 命令行入口
- [ ] 9.4 验证: 运行整个集合，检查结果汇总

---

## 阶段 10: 脚本与测试（远期）

> **目标**: 对标 Postman Pre-request Scripts/Tests，Hoppscotch 和 Bruno 的脚本能力。

- [ ] 10.1 评估 Rust 嵌入脚本引擎方案: Rhai / mlua / deno_core
- [ ] 10.2 实现 Pre-request Script 编辑器和执行
- [ ] 10.3 实现 Tests 脚本编辑器和断言 API (`pm.test`, `pm.expect`)
- [ ] 10.4 实现测试结果展示面板
- [ ] 10.5 验证: 编写测试脚本，断言成功/失败均有正确反馈

---

## 阶段 11: 多协议支持（远期）

> **目标**: 扩展协议支持，对标 Hoppscotch 的多协议覆盖度。

- [ ] 11.1 GraphQL 支持: 内省查询、Schema 浏览、查询编辑器（参考 Insomnia 的 GraphQL 体验）
- [ ] 11.2 WebSocket 支持: 连接管理、消息发送/接收面板（参考 Hoppscotch 的 WebSocket UI）
- [ ] 11.3 SSE (Server-Sent Events) 支持
- [ ] 11.4 gRPC 支持评估: 依赖 protobuf 编译，标记为探索性任务（参考 Yaak 和 Bruno 的 gRPC 实现）

---

## 阶段 12: 插件与扩展系统（远期）

> **目标**: 对标 Yaak 的插件系统，开放认证、模板标签和 UI 定制能力。

- [ ] 12.1 设计插件加载模型: 沙箱、生命周期、权限控制
- [ ] 12.2 实现认证插件接口: 自定义认证类型注册
- [ ] 12.3 实现模板标签插件: 自定义 `{{tag}}` 处理器
- [ ] 12.4 设计 UI 主题扩展点
- [ ] 12.5 验证: 加载自定义插件，认证和模板标签正常工作

---

## 阶段 13: 打磨与发布

> **目标**: UI 细节对齐与调研对标、稳定性和性能优化、文档完备，准备首次发布。
>
> **UI 持续优化原则**: 本阶段是对前序各阶段 UI 对齐检查（3.10/5.7/6.8）的汇总校验。
> 每个功能模块应在实现阶段当即完成 UI 对齐，不把 UI 债堆积到最后一次性修。

### 13.1 UI 系统完整性审查（参照: `docs/RESEARCH_UI.md` + `docs/RESEARCH_INTERACTION.md`）
- [ ] 13.1.1 颜色审查: 全局表面色/边框色/强调色一致、HTTP 方法色映射（如 1.2/2.3 定义）无硬编码色值、响应状态色调（绿/红/琥珀/灰）全程统一
- [ ] 13.1.2 字体排版审查: 正文/代码字体一致、字号层级不乱跳、monospace 覆盖 URL/文件路径/API 路径/代码体/本地服务地址、placeholder 字体族与输入值一致
- [ ] 13.1.3 间距与尺寸审查: 标签页行 40px 固定高度 + 2px 活跃下划线；请求地址栏 36px 在 52px 工具带内显式 y 偏移；响应状态元数据 260px 右对齐 14px 右内边距；侧边栏选中路由 3px 主色左边标记；导入弹出窗口 520×58px
- [ ] 13.1.4 控件样式审查: 按钮主色/次要/危险/禁用状态由共享 UI 辅助函数统一管理；禁用控件无指针手势；侧边栏 HTTP 方法为定宽文本非填充徽章；弹出窗口对齐触发控件原点
- [ ] 13.1.5 交互规范审查: 前置条件不满足时动作禁用且 UI 可见表达；导入新规格自动停止运行中的 Mock 服务；过滤仅改变视图不改变全局操作计数；单行输入 Enter 触发主动作；不预埋未实现功能的标签或控件

### 13.2 竞品对标审查（参照: `docs/BENCHMARK.md` + `docs/RESEARCH_FEATURES.md`）
- [ ] 13.2.1 核心交互流畅度对标 Hoppscotch: 路由选择→请求发送→响应展示的往返延迟
- [ ] 13.2.2 功能深度对标 Postman 核心: Headers/Body/Auth 编辑器完备度、响应查看器 Pretty/Raw/Headers 切换
- [ ] 13.2.3 原生体验对标 Yaak: 启动时间、内存占用、离线能力
- [ ] 13.2.4 更新 `docs/BENCHMARK.md` 对标报告至最新状态，确保阶段 0 调研结论反映到当前产品状态

### 13.3 性能与稳定性
- [ ] 13.3.1 大规格文件导入流畅度优化（目标：1000+ 路由的 OpenAPI 文件 <2s 解析渲染）
- [ ] 13.3.2 响应体渲染性能优化（大 JSON 响应的增量渲染和滚动性能）
- [ ] 13.3.3 启动时间优化（目标 <1s）
- [ ] 13.3.4 错误处理完善: 网络错误、文件解析错误、Mock 启动失败的用户友好提示
- [ ] 13.3.5 可执行文件大小优化（目标 ~10 MB）

### 13.4 文档体系
- [ ] 13.4.1 编写用户使用指南 `docs/USER_GUIDE.md`: 安装、导入规格、发送请求、启动 Mock、变量/环境、集合管理
- [ ] 13.4.2 编写开发者文档 `docs/DEV_GUIDE.md`: 项目架构、模块职责、构建流程、贡献指南
- [ ] 13.4.3 维护 `docs/DESIGN.md` 至最新设计决策状态：每次设计方向变化或重复决策出现时即时记录
- [ ] 13.4.4 维护 `docs/PRD.md` 至最新产品方向，更新 Backlog 列表
- [ ] 13.4.5 维护 `docs/TODO.md`（本文档）的任务状态与附录准确

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
| `docs/BENCHMARK.md` | 对标报告与差异化定位 | 0.5 |
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
| 环境变量 | 核心已实现，环境创建 UI 待补齐 | — |
| 集合系统 | 数据模型已实现，UI 待开发 | — |
| 请求历史 | 数据模型与核心逻辑已实现，UI 待验证 | — |
| 代码生成 | 核心已实现，待端到端验证 | — |
| 集合运行器 | 远期 | — |
| 脚本/测试 | 远期 | — |
| 多协议 | 远期 | — |
| 插件系统 | 远期 | — |

## 附录 C: 竞品速查表

| 维度 | Postman | Hoppscotch | Bruno | Insomnia | Yaak | **ZenAPI 目标** |
|------|---------|------------|-------|----------|------|----------------|
| 许可证 | 专有 | MIT | MIT | Apache 2.0 | 专有(源码) | 待定 |
| 技术栈 | Electron | Vue+PWA | Electron | Electron | Tauri+Rust | **GPUI+Rust** |
| 应用体积 | ~400MB | <10MB | ~200MB | ~200MB | ~15MB | **~10MB** |
| 启动时间 | 3-5s | <1s | 2-3s | 2-3s | <1s | **<1s** |
| Mock Server | ✅ | ❌ | ❌ | ✅ | ❌ | **✅ 原生** |
| OpenAPI 深度 | ✅ | ❌ | ❌ | ✅ | ❌ | **✅ 原生** |
| 离线优先 | ⚠️ | ✅ | ✅ | ⚠️ | ✅ | **✅** |
| 多协议 | REST/GQL/WS/gRPC | REST/GQL/WS/MQTT/SSE | REST/GQL/gRPC/WS | REST/GQL/gRPC/WS/SSE | REST/GQL/WS/SSE/gRPC | REST(首期) → 扩展 |
| 插件系统 | ✅ | ❌ | ❌ | ✅ | ✅ | 远期 |
| Git 集成 | ⚠️ 云同步 | ❌ | ✅ 原生 | ✅ | ❌ | 远期 |
