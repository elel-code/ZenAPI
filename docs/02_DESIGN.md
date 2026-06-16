# ZenAPI Design Notes

本文档记录 ZenAPI 的视觉和交互设计决策，作为开发迭代的持续参考。
当 UI 方向变更或产品决策明确时，及时更新本文档。

## 设计方向

ZenAPI 的视觉语言定义为 **"Geek Modernity"** —— 面向 API 架构师和开发者的高性能美学。
品牌个性：技术性、精确、透明。避免不必要的装饰，以结构清晰和信息密度为核心。

风格来自 **现代极简主义** 和 **开发者工具美学**。内容（尤其是代码和数据）是界面的主要驱动力，
情感感受是 "可控的力量"：高效、低延迟、高度组织化的环境。

### 设计原则

- **暗色优先**: 默认暗色主题，减少长时间技术工作时的眼部疲劳。
- **高密度信息**: 适合宽屏显示器和多列分屏视图的工作站级别信息密度。
- **代码即一等公民**: 所有技术数据（API 路径、JSON 响应、终端输出）使用等宽字体。
- **色彩克制**: 通过色调层次而非重阴影传达深度，主色仅用于关键交互和状态指示。
- **工具美学**: 界面应该像一个专业工具，而非营销网站或通用桌面应用。

## Slint UI 框架策略

ZenAPI 的桌面 UI 框架为 **Slint**。此前的 GPUI 实验代码不是兼容接口。

- Slint 文件（`.slint`）是 UI 定义的主要载体。
- 使用 `slint` crate（v1）作为运行时，`slint-build` 作为构建时编译器。
- 所有 UI 组件、布局、样式和交互行为通过 `.slint` 文件声明式定义。
- Rust 端负责业务逻辑、状态管理、数据模型、网络请求和 Mock 服务器。
- 通过 Slint 的 `global` 和 `callback` 机制连接 Rust 业务逻辑与 UI。
- 保持可复用的产品逻辑（OpenAPI 解析、请求传输、Mock 服务器）在纯 Rust 模块中。
- 不要引入 GPUI 兼容层、适配器或桥接代码。

## 颜色系统

所有颜色值来自 `stitch_nextgen_api_studio/nexus_api/DESIGN.md` 定义的设计系统。
在 Slint 中应定义为全局颜色常量或 palette `export`。

### 表面色 (Surface)

| Token | 色值 | 用途 |
|-------|------|------|
| `background` | `#13131b` | 最深层：应用背景 |
| `surface` | `#13131b` | 主表面 |
| `surface-dim` | `#13131b` | 暗淡表面 |
| `surface-bright` | `#393841` | 亮表面 |
| `surface-container-lowest` | `#0d0d15` | 最深容器 |
| `surface-container-low` | `#1b1b23` | 深容器 |
| `surface-container` | `#1f1f27` | 默认容器（卡片/面板） |
| `surface-container-high` | `#292932` | 高亮容器 |
| `surface-container-highest` | `#34343d` | 最高容器 |
| `surface-variant` | `#34343d` | 变体表面 |
| `inverse-surface` | `#e4e1ed` | 反转表面 |
| `inverse-on-surface` | `#303038` | 反转表面上文字 |

### 主色 (Primary) — Vibrant Indigo

| Token | 色值 | 用途 |
|-------|------|------|
| `primary` | `#c0c1ff` | 主操作、激活导航、焦点指示 |
| `on-primary` | `#1000a9` | 主色上的文字 |
| `primary-container` | `#8083ff` | 主色容器 |
| `on-primary-container` | `#0d0096` | 主色容器上的文字 |
| `primary-fixed` | `#e1e0ff` | 固定主色 |
| `primary-fixed-dim` | `#c0c1ff` | 暗淡固定主色 |
| `on-primary-fixed` | `#07006c` | 固定主色上的文字 |
| `on-primary-fixed-variant` | `#2f2ebe` | 固定主色变体上的文字 |
| `inverse-primary` | `#494bd6` | 反转主色 |
| `surface-tint` | `#c0c1ff` | 表面着色 |

### 次要色 (Secondary) — Cyber Mint

| Token | 色值 | 用途 |
|-------|------|------|
| `secondary` | `#4edea3` | 成功状态、活跃 API 端点、"可用" 指示器 |
| `on-secondary` | `#003824` | 次要色上的文字 |
| `secondary-container` | `#00a572` | 次要色容器 |
| `on-secondary-container` | `#00311f` | 次要色容器上的文字 |
| `secondary-fixed` | `#6ffbbe` | 固定次要色 |
| `secondary-fixed-dim` | `#4edea3` | 暗淡固定次要色 |
| `on-secondary-fixed` | `#002113` | 固定次要色上的文字 |
| `on-secondary-fixed-variant` | `#005236` | 固定次要色变体上的文字 |

### 第三色 (Tertiary) — 警告/强调

| Token | 色值 | 用途 |
|-------|------|------|
| `tertiary` | `#ffb783` | 警告、强调操作 |
| `on-tertiary` | `#4f2500` | 第三色上的文字 |
| `tertiary-container` | `#d97721` | 第三色容器 |
| `on-tertiary-container` | `#452000` | 第三色容器上的文字 |
| `tertiary-fixed` | `#ffdcc5` | 固定第三色 |
| `tertiary-fixed-dim` | `#ffb783` | 暗淡固定第三色 |
| `on-tertiary-fixed` | `#301400` | 固定第三色上的文字 |
| `on-tertiary-fixed-variant` | `#703700` | 固定第三色变体上的文字 |

### 错误色 (Error)

| Token | 色值 | 用途 |
|-------|------|------|
| `error` | `#ffb4ab` | 错误状态、危险操作 |
| `on-error` | `#690005` | 错误色上的文字 |
| `error-container` | `#93000a` | 错误容器 |
| `on-error-container` | `#ffdad6` | 错误容器上的文字 |

### 表面文字 (On-surface)

| Token | 色值 | 用途 |
|-------|------|------|
| `on-surface` | `#e4e1ed` | 主文字色 |
| `on-surface-variant` | `#c7c4d7` | 次要文字色 |
| `on-background` | `#e4e1ed` | 背景上的文字 |
| `outline` | `#908fa0` | 轮廓/边框 |
| `outline-variant` | `#464554` | 轮廓变体/细分隔线 |

### HTTP 方法色

| 方法 | 色值 | 类名参考 |
|------|------|----------|
| GET | `#3b82f6` (Blue-500) | 蓝色 |
| POST | `#22c55e` (Green-500) | 绿色 |
| PUT | `#eab308` (Yellow-500) | 黄色 |
| PATCH | `#eab308` (Yellow-500) | 黄色 |
| DELETE | `#ef4444` (Red-500) | 红色 |
| OPTIONS | `#908fa0` (outline) | 灰色 |
| HEAD | `#908fa0` (outline) | 灰色 |

### 使用原则

- 主色（Indigo）仅用于：主操作按钮、激活导航状态、焦点边框、选中标记。
- 次要色（Mint）仅用于：成功状态、活跃端点、Mock 服务器运行指示器。
- 第三色仅用于：警告状态、破坏性操作的确认态。
- 颜色传达语义：不在非交互表面上大面积铺色。
- 深度通过色调层次（surface → surface-container → surface-container-high）而非阴影传达。

## 字体排版

字体分为两个体系：Inter 用于 UI 界面，JetBrains Mono 用于技术数据。

### 字体族

| 用途 | 字体 | 后备 |
|------|------|------|
| UI 正文 | Inter | system-ui, sans-serif |
| 代码/技术数据 | JetBrains Mono | monospace |
| 图标 | Material Symbols Outlined | — |

### 字号层级

| Token | 字号 | 字重 | 行高 | 用途 |
|-------|------|------|------|------|
| `display-lg` | 48px | 700 | 1.2 | 大标题（少用） |
| `headline-lg` | 32px | 600 | 1.3 | 页面标题 |
| `headline-md` | 24px | 600 | 1.4 | 面板标题 |
| `body-lg` | 16px | 400 | 1.6 | 正文、主要文本 |
| `body-sm` | 14px | 400 | 1.5 | 辅助文本、标签 |
| `code-md` | 14px | 400 | 1.7 | 代码块、API 路径、JSON |
| `code-sm` | 12px | 400 | 1.6 | 小号代码、元数据 |
| `label-caps` | 12px | 700 | 1.0 | 表头、标签徽章 |

### 排版规则

- 所有 API 端点、JSON 载荷、终端输出必须使用 JetBrains Mono。
- UI 导航、标签、按钮文字使用 Inter。
- 代码块保持较大行高 (1.7) 以确保嵌套对象可读。
- `label-caps` 样式用于表头和小型元数据标签，以区别于可交互正文。
- 使用 `letter-spacing: 0.05em` 的 `label-caps` 样式。
- 不使用视口缩放字号；每种组件类型的字号固定。
- 用省略号截断长路径、状态和摘要文本。
- 缺失的可选元数据不留填充占位文字，保持行高稳定即可。

## 布局与间距

### 网格系统

- 使用 12 列流式网格，每个 gutter 20px。
- 最大内容宽度 1440px。
- 导航固定在可折叠左侧边栏（240px 宽度），最大化代码编辑器和文档的水平空间。

### 间距 scale

| Token | 尺寸 | 用途 |
|-------|------|------|
| `xs` | 4px | 极小间距、图标内边距 |
| `sm` | 8px | 组件内间距（按钮组、输入框组） |
| `md` | 16px | 主容器内边距、面板间距 |
| `lg` | 24px | 大间距 |
| `xl` | 32px | 超大间距 |
| `gutter` | 20px | 网格列间隙 |

### 布局结构

- **顶部栏**: 品牌区（72px 宽）+ Import 按钮 + Mock 控制 + 全局操作，高度紧凑。
- **左侧边栏**: 240px 可折叠导航，包含 Routes/Saved/History 标签页切换。
- **主工作区**: 请求编辑器（上半部分）+ 响应查看器（下半部分），可通过分隔条调整。
- **底部状态栏**: Mock 状态、路由计数、当前操作状态。

### 间距规则

- 主容器使用 16px (md) 内边距。
- 组件内部（按钮组、输入框、表格单元格）使用 8px (sm) 间距。
- 按钮高度保持在 34-40px。
- 列表行高度根据内容类型：文件夹/根行 32px，请求行 42px，结果/日志行 34px。

## 圆角

形状语言为 "Soft-Technical"（软技术风格）。

| Token | 尺寸 | 用途 |
|-------|------|------|
| `sm` | 4px | 紧凑列表行 |
| `DEFAULT` | 8px | 标准元素（按钮、输入框、卡片） |
| `md` | 12px | — |
| `lg` | 16px | 大型容器、模态框、主内容区 |
| `xl` | 24px | — |
| `full` | 9999px | 药丸形状 |

## 深度与层级

深度通过 **色调层次** 和 **细微轮廓线** 传达，而非重阴影。

| 层级 | 背景色 | 边框 | 用途 |
|------|--------|------|------|
| Level 0 | `#13131b` | — | 应用背景（最深） |
| Level 1 | `#1f1f27` | 1px `#464554` | 卡片/面板 |
| Level 2 | `#1f1f27` | 1px `#908fa0` | 弹出层/模态框（加阴影） |

弹出层/模态框附加阴影：`0px 10px 15px -3px rgba(0, 0, 0, 0.5)`。

交互悬停时，卡片应增加边框亮度，而非改变层级或阴影。

## 组件规范

### 按钮

- **主按钮**: 纯色 Indigo (`primary-container`) 填充，`on-primary-container` 色文字，8px 圆角。
- **次要按钮**: "Ghost" 风格 — 仅轮廓线 (`outline-variant`)，无背景填充。
- **危险按钮**: 使用 `error` 色边框和文字，白色/透明背景。
- **禁用按钮**: `outline-variant` 边框，降低不透明度文字，无交互。

### HTTP 方法标记

- 高对比度背景 + 粗体白色文字。
- GET: `#3b82f6` 背景，白色文字。
- POST: `#22c55e` 背景，白色文字。
- PUT/PATCH: `#eab308` 背景，深色文字。
- DELETE: `#ef4444` 背景，白色文字。
- 作为紧凑标签出现于路由列表、请求地址栏左侧。

### 输入框

- 背景色深于表面色 (`surface-container-lowest`: `#0d0d15`)。
- 1px 边框使用 `outline-variant` (`#464554`)。
- 文字使用 JetBrains Mono（代码内容）或 Inter（普通文本）。
- 聚焦状态：2px `primary` 色边框。
- 占位文字使用 `outline` (`#908fa0`)。
- 禁用状态：边框变暗，文字变灰，不可编辑。

### 请求地址栏

- 组合控件：方法选择器 + URL 输入 + 发送按钮。
- 方法选择器为左侧段，固定 100px 宽度。
- URL 输入为中间段，弹性宽度。
- 发送按钮为右侧段，Indigo 主色。
- 整体高度 40px，外壳 2px 边框。
- 请求进行中时，整个组合控件统一禁用。

### API 端点卡片

- 用于展示 API 路由。
- 头部包含方法标记（左侧）+ 路径（JetBrains Mono）+ 展开箭头（右侧）。
- 展开后显示完整文档：参数、请求体、响应 schema。

### 代码编辑器

- 深色背景（`surface-container-lowest`: `#0d0d15`）。
- 语法高亮使用 JetBrains Mono。
- 右上角悬停时显示 "复制" 图标按钮。

### 数据表格

- 带边框的行，无斑马条纹。
- 悬停行使用 `surface-container-high` (`#292932`) 高亮。
- 表头使用 `label-caps` 样式。

### 标签页

- 水平标签行，固定高度。
- 激活标签使用 `primary` 色底部下划线（2px）。
- 非激活标签使用 `on-surface-variant` 色文字。
- 标签文字使用 Inter 字体。

### 侧边栏导航

- 240px 固定宽度。
- 导航项：图标 + 文字标签，圆角 8px。
- 激活项：`primary-container` 背景 + `on-primary-container` 文字。
- 非激活项：`on-surface-variant` 文字，悬停时 `surface-variant` 背景。
- 图标使用 Material Symbols Outlined，激活时 `FILL=1`。
- 分组之间用 1px `outline-variant` 分隔线。

## Slint 实现指南

### 颜色定义

在 Slint 中定义全局颜色常量：

```slint
export global Theme {
    // Surface
    out property <color> background: #13131b;
    out property <color> surface: #13131b;
    out property <color> surface-container: #1f1f27;
    out property <color> surface-container-lowest: #0d0d15;
    out property <color> surface-container-high: #292932;
    out property <color> surface-variant: #34343d;

    // Primary
    out property <color> primary: #c0c1ff;
    out property <color> on-primary: #1000a9;
    out property <color> primary-container: #8083ff;
    out property <color> on-primary-container: #0d0096;

    // Secondary
    out property <color> secondary: #4edea3;
    out property <color> on-secondary: #003824;

    // Tertiary
    out property <color> tertiary: #ffb783;

    // Error
    out property <color> error: #ffb4ab;
    out property <color> on-error: #690005;

    // Text
    out property <color> on-surface: #e4e1ed;
    out property <color> on-surface-variant: #c7c4d7;

    // Outline
    out property <color> outline: #908fa0;
    out property <color> outline-variant: #464554;
}
```

### 字体定义

Slint 中通过 `import` 引入字体：

```slint
import { Inter, JetBrainsMono } from "fonts";
```

或者在系统级别配置后备字体族。在 `.slint` 文件中使用 `font-family` 属性指定。

### 组件模板

重点关注可复用组件的声明：

- `MethodChip` — HTTP 方法标记组件
- `AddressBar` — 组合请求地址栏
- `KeyValueEditor` — 键值对编辑器（Headers/Params/Vars 共用）
- `JsonViewer` — JSON 响应查看器
- `CodeBlock` — 代码展示/编辑器
- `TabBar` — 标签页切换
- `SidebarNav` — 侧边栏导航项
- `DataTable` — 通用数据表格

### 响应式布局

- 侧边栏支持折叠/展开。
- 请求/响应区域支持拖拽调整分割比例。
- 最小窗口宽度保证核心功能可用。

## 控件交互规范

### 文件导入

- Import 按钮点击后弹出文件路径输入浮层。
- 支持 Enter 确认和 Escape 关闭。
- 导入成功后，路由列表刷新，运行中的 Mock 服务自动停止。

### 请求发送

- URL 输入框中按 Enter 发送。
- Send 按钮点击发送。
- 发送期间，地址栏整体禁用。
- 加载状态通过 Send 按钮的禁用态 + 底部状态栏指示传达。

### Mock 服务器

- 顶部栏 Mock 开关：一键启动/停止。
- 运行状态使用 `secondary` (Mint) 色指示。
- 停止状态使用 `outline` 色。
- 端口号在底部状态栏紧凑显示。
- Mock 请求日志在底部面板展示。

### 集合操作

- 右键菜单：新建/重命名/删除/复制。
- 拖拽排序移动请求和文件夹。
- 导入/导出支持 ZenAPI 原生 JSON 格式。

### 变量替换

- 所有文本输入区域支持 `{{variableName}}` 语法。
- 变量预览：输入 `{{` 时触发自动补全。
- 未定义变量显示警告色 `tertiary`。

### 快捷键

| 快捷键 | 动作 |
|--------|------|
| Enter (URL) | 发送请求 |
| Ctrl+Enter | 从编辑器上下文发送 |
| Ctrl+F | 聚焦侧边栏搜索 |
| Ctrl+L | 聚焦 URL 输入 |
| Ctrl+S | 保存到集合 |
| Ctrl+1..7 | 切换请求编辑器标签页 |
| Ctrl+Shift+1..3 | 切换响应查看器标签页 |
| Escape | 关闭弹出层/菜单 |

## 图标系统

使用 Material Symbols Outlined 图标体系：

- 导航图标：`dashboard`, `send`, `cloud_queue`, `description`, `settings`, `variables`.
- 操作图标：`add`, `close`, `delete`, `content_copy`, `search`, `unfold_more`.
- 状态图标：`check_circle`, `error`, `warning`, `play_arrow`, `stop`.
- 图标尺寸：导航 20px，操作按钮 18px，内联 16px。
- 激活态使用 `FILL=1` 变体，非激活使用 `FILL=0`。

## 空状态与错误提示

- 空面板显示简洁的状态行，如 `No routes`、`No response`、`No history`。
- 不堆叠说明性文字，因为周围控件已表达下一步操作。
- 错误反馈在 Response 面板中显示，包含操作上下文、目标路径/URL、底层错误和修复提示。
- 底部状态栏仅在非空闲时显示（busy/running/error），空闲时隐藏默认 `Ready` 等填充文字。

## 模块级交互流程

### 全局导航流

```
用户启动 → TopBar (固定)
           ├── Import 按钮 → 弹出文件路径输入 → 导入 OpenAPI → Routes 列表刷新
           ├── Mock 开关 → 启动/停止 Mock 服务器 → 状态栏更新端口/状态色
           └── 搜索按钮 → 全局搜索（远期）

侧边栏导航 → 主内容区页面切换:
  Dashboard ←→ Request Builder ←→ Mock Manager ←→ API Docs
  次级: Settings ←→ Environments ←→ Team ←→ API Keys
```

### 页面内交互流

**Request Builder**:
```
选择路由/输入 URL → 设置 Method → 填写 Params/Headers/Body/Auth
   → 点击 Send / Enter → 地址栏整体 disabled → reqwest 发送
   → Response 面板更新 (status + time + size + body)
   → History 自动记录
```

**Mock Manager**:
```
选择端点 → 配置 Status Code/Delay/Response Body
   → 配置 Routing Rules (条件匹配)
   → 顶部 Mock 开关启动 → secondary 色状态指示
   → Live Traffic Log 实时记录到达的请求
```

**Environments**:
```
选择环境 → 编辑变量表 (Key/Value/Scope)
   → 变量值在 Request Builder 中通过 {{var}} 引用
   → 发送请求时自动替换
```

**Dashboard**:
```
打开应用 → Dashboard 为默认首页
   → 显示 KPI 概览（Uptime / P99 Latency / Success Rate）
   → 流量柱状图（最近 24H）
   → 端点排名表（按 RPM 排序）
   → 右侧 Quick Actions + Live Activity Feed
```

### 状态管理流

```
Rust AppState (单一数据源)
  ├── routes: Vec<Route>         → Sidebar Routes 列表
  ├── active_request: Request    → Request Builder 编辑器
  ├── response: Option<Response> → Response 面板
  ├── mock_running: bool         → TopBar Mock 开关 + StatusBar 端口
  ├── active_page: Page          → 主内容区页面切换
  ├── environments: EnvStore     → Environments 页面
  ├── collections: Collection    → Sidebar Saved 标签
  └── history: Vec<HistoryEntry> → Sidebar History 标签
```

数据流向：`用户操作 → Slint callback → Rust 处理 → 更新 AppState → Slint property 绑定自动刷新 UI`

### 错误处理流

```
操作失败 → Response 面板显示错误信息 (上下文 + 路径/URL + 底层错误 + 修复提示)
  ├── Import 失败 → Response 面板 + 状态保留旧 routes
  ├── Send 失败   → Response 面板 (status 显示错误, body 显示详情)
  ├── Mock 失败   → StatusBar 显示短标签 + Mock 开关置灰
  └── Collection 导入/导出失败 → Collection 状态行更新
```

## 迁移记录

- **2026-06-17**: 从 GPUI 切换回 Slint；设计系统全面对齐 Nexus API 暗色主题规范。旧版浅色 GPUI 设计笔记存档于本次提交历史。
