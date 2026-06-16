# ZenAPI Slint 组件设计规范

> 本文档定义 ZenAPI Slint UI 的所有可复用组件，基于 `stitch_nextgen_api_studio/` 下的
> Nexus API 设计系统。每个组件包含：视觉规格、尺寸约束、颜色映射、状态定义，
> 以及从设计 HTML 到 Slint 声明式组件的映射说明。

## 组件总览

| 组件名称 | Slint 类型 | 用途 | 复用模块 |
|----------|-----------|------|----------|
| `MethodChip` | `component` | HTTP 方法标记 | 全模块 |
| `StatusIndicator` | `component` | 状态圆点指示器 | Mock, Dashboard |
| `AddressBar` | `component` | 请求地址栏（方法+URL+发送） | Request Builder |
| `TabBar` | `component` | 标签页切换 | 全模块 |
| `KeyValueTable` | `component` | 键值对编辑表格 | Request, Env |
| `CodeEditor` | `component` | 代码编辑/预览区 | Request, Mock, Response |
| `DataTable` | `component` | 数据表格 | Dashboard, Mock, Settings |
| `SidebarNav` | `component` | 侧边栏导航 | 全模块（全局） |
| `KPICard` | `component` | KPI 指标卡片 | Dashboard, Analytics |
| `SearchInput` | `component` | 搜索/过滤输入框 | 全模块 |
| `Dropdown` | `component` | 下拉选择器 | Request, Mock, Env |
| `Toggle` | `component` | 开关控件 | Mock, Settings |
| `EmptyState` | `component` | 空状态占位 | 全模块 |
| `Toast` | `component` | 操作反馈提示 | 全模块 |

---

## 1. MethodChip — HTTP 方法标记

### 视觉规格

- **高度**: 22px（`py-1` = 4px 上下内边距）
- **水平内边距**: 8px（`px-2`）
- **字体**: JetBrains Mono, 12px（`font-code-sm`）, 粗体 700
- **圆角**: 4px（`rounded`）
- **边框**: 1px，半透明方法色

### 方法色映射

| 方法 | 背景色 | 文字色 | 边框色 |
|------|--------|--------|--------|
| GET | `#3b82f6` / 30% 透明度 | `#60a5fa` (blue-400) | `#3b82f6` / 50% |
| POST | `#22c55e` / 30% 透明度 | `#4ade80` (green-400) | `#22c55e` / 50% |
| PUT | `#eab308` / 30% 透明度 | `#facc15` (yellow-400) | `#eab308` / 50% |
| PATCH | `#eab308` / 30% 透明度 | `#facc15` (yellow-400) | `#eab308` / 50% |
| DELETE | `#ef4444` / 30% 透明度 | `#f87171` (red-400) | `#ef4444` / 50% |
| OPTIONS | `#908fa0` / 20% 透明度 | `#908fa0` (outline) | `#908fa0` / 30% |
| HEAD | `#908fa0` / 20% 透明度 | `#908fa0` (outline) | `#908fa0` / 30% |

### Slint 组件设计

```slint
// 伪代码，非实际 Slint 语法
component MethodChip {
    in property <string> method;
    // 内部根据 method 值切换颜色
    // 显示 method 文本（大写），JetBrains Mono 12px
}
```

### 使用场景

- 侧边栏路由列表：每行左侧
- 请求地址栏：URL 输入框左侧段
- API 文档：端点标题旁
- Mock 管理：端点列表行
- Dashboard：端点排名表

---

## 2. StatusIndicator — 状态圆点

### 视觉规格

- **尺寸**: 8×8px（`w-2 h-2`）
- **形状**: 圆形（`rounded-full`）
- **状态色映射**:

| 状态 | 颜色 Token | 色值 | 附加样式 |
|------|-----------|------|----------|
| 活跃/在线/成功 | `secondary` | `#4edea3` | — |
| 非活跃/离线/停止 | `outline-variant` | `#464554` | — |
| 错误/告警 | `error` | `#ffb4ab` | — |
| 运行中 | `secondary` | `#4edea3` | 附加脉冲动画（CSS `animate-ping`） |

### Slint 组件设计

```slint
component StatusIndicator {
    in property <string> status; // "active", "inactive", "error", "pulsing"
    // 8x8px 圆形，根据状态设置背景色
    // pulsing 状态：主圆点 + 放大淡出动画环
}
```

### 使用场景

- Dashboard: "All Systems Operational" 旁
- Mock Manager: 端点列表中每个端点的状态
- 侧边栏: 项目/环境状态指示

---

## 3. AddressBar — 请求地址栏

### 视觉规格

- **整体高度**: 40px
- **背景**: `surface-container` (`#1f1f27`)
- **边框**: 1px `outline-variant` (`#464554`), 聚焦时 2px `primary`
- **圆角**: 8px（`rounded`）

### 内部结构（三段式）

| 段 | 宽度 | 内容 |
|----|------|------|
| 方法选择器 | 100px 固定 | MethodChip + 下拉箭头（`unfold_more` 图标）|
| URL 输入 | 弹性（flex-1）| 单行文本输入，JetBrains Mono 14px |
| 发送按钮 | 自适应 | "Send" 文字 + `send` 图标，`inverse-primary` (`#494bd6`) 背景 |

### 分段细节

**方法选择器**:
- 左侧段，与 URL 输入之间用 1px `outline-variant` 分隔
- 点击弹出方法下拉菜单
- 菜单项高 30px，选中态 `surface-variant` 背景
- 仅在 URL 输入聚焦时，整个地址栏显示 `primary` 色聚焦边框

**URL 输入**:
- 背景: `surface-container-lowest` (`#0d0d15`)
- 占位文字: `outline` (`#908fa0`), "Enter request URL"
- Enter 键发送请求

**发送按钮**:
- 右侧段，`inverse-primary` 背景 + `on-primary-container` 文字
- 右侧圆角 8px，左侧无圆角（附着于地址栏外壳）
- 发送中状态: 整体禁用，文字变暗
- 悬停: 背景变亮 `#3d3fba`

### Slint 组件设计

```slint
component AddressBar {
    // 作为复合控件嵌入 Request 面板
    // 内部包含: method_selector (100px) + divider + url_input (flex) + send_button
    // 聚焦状态提升到外层边框
    callback send_request(string url, string method);
}
```

---

## 4. TabBar — 标签页切换

### 视觉规格

- **高度**: 34px
- **背景**: `surface-container-low` (`#1b1b23`)
- **底部边框**: 1px `outline-variant`

### 标签项规格

| 属性 | 激活态 | 非激活态 |
|------|--------|----------|
| 文字色 | `primary` (`#c0c1ff`) | `on-surface-variant` (`#c7c4d7`) |
| 底部指示线 | 2px `primary` | 2px 透明 |
| 字重 | 600 (medium) | 400 (normal) |
| 内边距 | `pb-sm` (底部 8px) | 同左 |
| 间距 | 16px (`gap-lg`) | 同左 |

### 两种实例

**请求编辑器标签页**: Params, Headers, Body, Auth, Scripts（5个标签）
**响应查看器标签页**: Body, Headers, Cookies（3个标签）

### 附加标记

- 标签文字右侧可有计数徽章（如 `Headers 8`），背景 `surface-variant`，文字 10px

### Slint 组件设计

```slint
component TabBar {
    in property <[string]> tabs;
    in-out property <int> active_index;
    // 水平排列标签
    // 点击切换 active_index，触发回调
}
```

---

## 5. KeyValueTable — 键值对编辑器

### 视觉规格

- **表头**: `label-caps` 样式 (Inter 12px/700)，文字 `on-surface-variant`
- **行高**: 34px
- **列布局**:

| 用途 | Key 列宽 | Value 列宽 | 操作列 |
|------|----------|-----------|--------|
| Headers | 128px | 弹性 | 30px（`+`/`x`） |
| Params | 128px | 弹性 | 30px |
| Body Form | 112px | 弹性 | 30px |
| Variables | 128px | 弹性 | 30px |
| Tests | 96px (Kind) | 弹性 (Target + Expect) | 30px |

### 交互规格

- **新增行**: 表头 `+` 按钮添加空行
- **删除行**: 每行 `x` 按钮（仅在该行可删除时显示）
- **批量操作**: Headers 支持批量粘贴（`key: value` 格式解析）
- **复制**: 将当前所有行格式化为 `key: value` 文本

### 表头标签

| 编辑器 | 表头标签 | 环境变量用 |
|--------|----------|------------|
| Headers | `Header` / `Value` | — |
| Params | `Param` / `Value` | — |
| Variables | `Var` / `Value` | `{{var}}` 语法 |
| Body (Form) | `Field` / `Value` | — |
| Tests | `Kind` / `Target` / `Expect` | — |

### Slint 组件设计

```slint
component KeyValueTable {
    in property <string> key_header: "Key";
    in property <string> value_header: "Value";
    in property <length> key_column_width: 128px;
    // 内部使用 TableView 或循环生成行
    // 支持 + 添加、x 删除
    // 支持变量 {{}} 语法高亮
}
```

---

## 6. CodeEditor — 代码编辑/预览区

### 视觉规格

- **背景**: `surface-container-lowest` (`#0d0d15`)
- **字体**: JetBrains Mono 14px (`font-code-md`), 行高 1.7
- **行号列**: 40px 宽，背景 `surface-container-lowest`，右对齐，文字 `on-surface-variant` 50%透明度
- **滚动条**: 6px 宽，thumb `#34343d`，track 透明

### 两种模式

**编辑模式**（Request Body 编辑器）:
- 可编辑文本框
- 语法高亮（JSON/XML/HTML）- property=#8083ff, string=#4edea3, number=#d97721
- 右上角悬停显示 "Format"（仅 JSON Raw 模式）+ "Copy" 按钮
- Body 类型切换器（none / form-data / raw / graphql / binary）在编辑器上方
- Raw 模式下子类型选择：JSON / XML / Text / HTML
- 最小高度 118px

**预览模式**（Response Body 查看器）:
- 只读文本框，可选可复制
- 与编辑模式相同的语法高亮
- 右上角 "Copy" 按钮（悬停显示）
- 支持 "Fold" / "Open" JSON 折叠

### Slint 组件设计

```slint
component CodeEditor {
    in property <bool> read_only: true;
    in property <string> language: "json"; // json, xml, text, html
    in-out property <string> content;
    // 行号列 + 代码区域
    // read_only 决定是否可编辑
    // Copy 按钮悬停显示
}
```

---

## 7. DataTable — 数据表格

### 视觉规格

- **表头**: `label-caps` 样式, `surface-container-lowest` 背景, `border-b border-outline-variant`
- **数据行**: `hover:bg-surface-variant`, `border-b border-outline-variant/30`
- **无斑马条纹**: 所有行统一背景
- **行高**: 40-42px（根据内容和缩进调整）

### 列配置

**端点排名表**（Dashboard）:
- 12 列网格, 20px 间距
- Method: 2/12, Path: 6/12, Latency: 2/12（右对齐）, RPM: 2/12（右对齐）

**Mock 端点表**:
- Method 列（含状态圆点）, Path 列, Status 列, Actions 列

**API Keys 表**:
- Name, Prefix, Created, Expiry, Last Used, Actions

### Slint 组件设计

```slint
component DataTable {
    // 列定义由父组件传入
    // 行数据通过 model 绑定
    // hover 高亮行
}
```

---

## 8. KPICard — 指标卡片

### 视觉规格

- **背景**: `surface-container` (`#1f1f27`)
- **边框**: 1px `outline-variant` (`#464554`)
- **圆角**: 8px（`rounded-lg`）
- **内边距**: 16px（`p-md`）
- **悬停**: 背景水印图标从 opacity-10 → opacity-20

### 内部布局

```
┌─────────────────────────────┐
│ [图标 16px] LABEL (caps)    │  ← 顶部: label-caps 样式 + 16px 状态图标
│                             │
│  MAIN VALUE                 │  ← 中间: display-lg (48px/700) 主数值
│  unit                       │  ← 单位用 headline-md (24px/600) 灰色
│                             │
│  subtitle link →            │  ← 底部: code-sm 副标题 + 链接
└─────────────────────────────┘
```

### 颜色变体

| 卡片类型 | 图标/边角色 | 示例 |
|----------|-----------|------|
| 默认 | `primary` (Indigo) | Uptime, P99 Latency |
| 成功 | `secondary` (Mint) | Success Rate |
| 警告 | `tertiary` (Orange) | Error Rate, Incidents |
| 错误 | `error` (Red) | 关键告警 |

### Slint 组件设计

```slint
component KPICard {
    in property <string> label;
    in property <string> value;
    in property <string> unit;
    in property <string> subtitle;
    in property <string> status; // "default", "success", "warning", "error"
    in property <image> icon;
    callback link_clicked();
}
```

---

## 9. SidebarNav — 侧边栏导航

### 视觉规格

- **宽度**: 240px（固定）
- **背景**: `surface-container-low` (`#1b1b23`)
- **右边框**: 1px `outline-variant`
- **内边距**: 16px（`p-md`）

### 内部结构

**用户区**（顶部）:
- 40×40px 头像（`rounded-lg`，`surface-container-highest` 背景）
- 用户名: `primary` 色，`headline-sm` (≈18px/600/粗体)
- 团队名: `on-surface-variant`，`body-sm` (14px)

**项目切换器**:
- 全宽按钮，`surface-container` 背景
- 文字: 项目名 + `unfold_more` 下拉图标
- 悬停: `surface-variant`

**主导航**（图标 + 文字标签，8px 圆角）:

| 导航项 | 图标 | 目标页面 |
|--------|------|----------|
| Dashboard | `dashboard` | Dashboard |
| Requests | `send` | Request Builder |
| Mocks | `cloud_queue` | Mock Manager |
| Docs | `description` | API Documentation |

**分隔线**（`h-px bg-outline-variant`），间距 `my-sm`

**次级导航**:

| 导航项 | 图标 | 目标页面 |
|--------|------|----------|
| Project Settings | `settings` | Settings |
| Environment Variables | `variables` | Environments |
| Team Members | `group` | Team |
| API Keys | `key` | API Keys |

**版本号**（底部）: `v2.4.0`，`on-surface-variant`，小字

### 导航项规格

| 属性 | 激活态 | 非激活态 |
|------|--------|----------|
| 背景 | `primary-container` (`#8083ff`) | 透明 |
| 文字色 | `on-primary-container` (`#0d0096`) | `on-surface-variant` (`#c7c4d7`) |
| 文字字重 | 700（粗体）| 500（中等）|
| 图标 FILL | 1（填充）| 0（轮廓）|
| 悬停背景 | — | `surface-variant` (`#34343d`) |
| 内边距 | `px-sm py-xs` | 同左 |
| 图标间距 | `gap-md` (16px) | 同左 |

### 移动端底部导航

- 仅在 `<md` 断点显示
- 固定底部，全宽，`surface-container` 背景
- 4 个标签: Dashboard, Requests, Mocks, Docs
- 每个标签: 垂直排列（图标 + label-caps 文字），居中
- 激活态: `primary-container` 背景，图标 FILL=1

### Slint 组件设计

```slint
component SidebarNav {
    in property <[NavItem]> primary_items;
    in property <[NavItem]> secondary_items;
    in-out property <int> active_index;
    callback item_selected(int index);
}
```

---

## 10. Dropdown — 下拉选择器

### 视觉规格

- **触发按钮**: 与所属容器相同背景, 1px `outline-variant` 边框, 8px 圆角
- **内边距**: `px-sm py-xs`
- **文字**: `body-sm` (Inter 14px)
- **图标**: `unfold_more` / `expand_more`，18px
- **弹出菜单**: `surface-container-high` 背景, 1px `outline` 边框, 阴影
- **菜单项高**: 30px
- **选中项**: `surface-variant` 背景
- **悬停项**: `surface-container-high` 背景

### 变体

| 变体 | 用途 | 宽度 |
|------|------|------|
| 方法选择 | 请求方法下拉 | 自适应（最小 100px） |
| 语言选择 | 代码生成语言 | 自适应（短标签） |
| 环境选择 | 环境切换 | 自适应 |
| Body 类型 | Raw 子类型 | 自适应 |

### Slint 组件设计

```slint
component Dropdown {
    in property <[string]> options;
    in-out property <int> selected_index;
    in property <length> min_width: 100px;
}
```

---

## 11. EmptyState — 空状态

### 视觉规格

- **文字**: `body-sm` (Inter 14px), `on-surface-variant` 色
- **对齐**: 居中对齐
- **内边距**: `p-md`

### 各模块空状态文案

| 模块 | 空状态 | 触发条件 |
|------|--------|----------|
| Routes | `No routes` | 未导入 OpenAPI 文件 |
| Saved | `No saved` | 集合为空 |
| History | `No history` | 无发送记录 |
| Response | `No response` | 未发送请求 |
| Codegen | `No URL` | 未输入 URL |
| Mock Log | `No logs` | 无 Mock 请求到达 |
| Runner | `No requests` / `No results` | 集合空或未运行 |
| WebSocket | `No messages` | 未连接或未收到消息 |
| SSE | `No events` | 未订阅或未收到事件 |

### Slint 组件设计

```slint
component EmptyState {
    in property <string> message;
    // 单行居中文字
}
```

---

## 12. Toast — 操作反馈

### 视觉规格

- **位置**: 右下角固定
- **背景**: `surface-container-high`
- **边框**: 1px `outline-variant`
- **圆角**: 8px
- **内边距**: `p-sm` ~ `p-md`
- **自动消失**: 3 秒后淡出

### 类型

| 类型 | 左边框色 | 图标 |
|------|---------|------|
| 成功 | `secondary` (Mint) | `check_circle` |
| 错误 | `error` (Red) | `error` |
| 警告 | `tertiary` (Orange) | `warning` |
| 信息 | `primary` (Indigo) | `info` |

### 使用场景

- Import 成功: "42 routes imported"
- 集合保存: "Request saved"
- 集合导出: "Exported as Postman v2.1"
- Mock 启动: "Mock running on :8080"
- 错误: "Connection refused"

### Slint 组件设计

```slint
component Toast {
    in property <string> message;
    in property <string> kind; // "success", "error", "warning", "info"
    // 自动 3s 消失
}
```

---

## 字体使用规范（重申）

- **UI 控件文字**: Inter, 14px `body-sm` / 16px `body-lg`
- **表头/标签**: Inter, 12px `label-caps`, 字间距 0.05em, 粗体 700
- **代码/API 路径/JSON**: JetBrains Mono, 14px `code-md` (行高 1.7) / 12px `code-sm`
- **数值/KPI**: Inter, 48px `display-lg` (行高 1.2, 字间距 -0.02em, 粗体 700)
- **页面标题**: Inter, 32px `headline-lg` (粗体 600)
- **面板标题**: Inter, 24px `headline-md` (粗体 600)

## 间距规范（重申）

- **组件内部**: 8px (`sm`)
- **容器内边距**: 16px (`md`)
- **面板间隔**: 24px (`lg`)
- **页面边距**: 20px (`gutter`)
- **网格列间隙**: 20px

## 圆角规范（重申）

- **按钮/输入框/卡片**: 8px
- **弹出层/模态框**: 16px (`lg`)
- **方法标记/徽章**: 4px (紧凑)
- **药丸/开关**: 9999px (`full`)

## 颜色 Token 引用

所有组件颜色应引用 `theme.slint` 中定义的全局 token，不硬编码色值。
详见 `docs/02_DESIGN.md` 的颜色系统章节。
