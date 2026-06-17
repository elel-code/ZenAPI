# ZenAPI 页面布局规范

> 本文档定义 ZenAPI 各页面的面板分割、尺寸约束、响应式行为和内容区域规格。
> 所有布局基于 `stitch_nextgen_api_studio/` 下各模块的 HTML 设计。

## 全局外壳

ZenAPI 采用单窗口多页面架构，全局外壳包含：

```
┌──────────────────────────────────────────────────┐
│ 顶部栏 (64px, 固定)                         [搜索] │
├────────┬─────────────────────────────────────────┤
│        │                                         │
│ 侧边栏 │         主内容区（页面切换）              │
│ 240px  │         弹性宽度，独立滚动               │
│        │                                         │
│        │                                         │
├────────┴─────────────────────────────────────────┤
│ 底部状态栏 (28px)    Mock:8080 │ 4 routes │ Busy   │
└──────────────────────────────────────────────────┘
移动端: 底部导航栏替代侧边栏
```

### 顶部栏

| 属性 | 规格 |
|------|------|
| 高度 | 64px |
| 背景 | `surface-dim` (`#13131b`) |
| 底部边框 | 1px `outline-variant` |
| 左侧 | 菜单按钮 + "API Architect" 品牌文字（`headline-md`, `primary` 色） |
| 右侧 | 搜索图标按钮 |
| 内边距 | `px-md` (16px) |

### 侧边栏

| 属性 | 规格 |
|------|------|
| 宽度 | 240px（固定） |
| 背景 | `surface-container-low` (`#1b1b23`) |
| 右边框 | 1px `outline-variant` |
| 显示 | `≥md` 断点显示，`<md` 隐藏（用底部导航替代） |
| 滚动 | 独立垂直滚动 |

内部结构详见 `docs/04_COMPONENTS.md` 的 SidebarNav 组件规范。

### 底部状态栏

| 属性 | 规格 |
|------|------|
| 高度 | 28px |
| 背景 | `surface-container` |
| 顶部边框 | 1px `outline-variant` |
| 左侧 | Mock 服务状态（端口号，如 `:8080`） |
| 中间 | 路由计数（如 `4 routes` 或 `2/4`） |
| 右侧 | 当前操作状态（`Busy` / 错误信息，最大 220px，空闲时隐藏） |

### 移动端底部导航

| 属性 | 规格 |
|------|------|
| 显示 | `<md` 断点 |
| 高度 | 56px |
| 背景 | `surface-container` |
| 顶部边框 | 1px `outline-variant` |
| 标签 | 4 个：Dashboard、Requests、Mocks、Docs |
| 激活态 | `primary-container` 背景，图标 FILL=1 |

---

## 页面布局详解

### 1. Dashboard（仪表盘）

```
┌────────┬──────────────────────────────────┬──────────┐
│        │  System Overview                 │  QUICK   │
│        │  ● All Systems Operational       │ ACTIONS  │
│        │                                  │          │
│        │ ┌──────┐ ┌──────┐ ┌──────┐     │ Rotate   │
│ 侧边栏 │ │Uptime│ │P99  │ │Succ. │     │ Keys     │
│        │ │99.99%│ │42ms │ │99.7% │     │          │
│ 240px  │ └──────┘ └──────┘ └──────┘     │ Webhooks │
│        │                                  │          │
│        │ Traffic Volume (chart)           │ ──────── │
│        │ ▂▃▅▇█▇▅▃▂▃▅▇█                   │ LIVE     │
│        │                                  │ ACTIVITY │
│        │ Top Endpoints (table)            │ ● Deploy │
│        │ GET  /v2/users/...  32ms  4.2k  │ ● Alert  │
│        │ POST /v2/trans...  145ms  1.8k  │ ● Webhook│
│        │ PUT  /v2/inven...   89ms   950  │          │
│        │                                  │          │
└────────┴──────────────────────────────────┴──────────┘
```

| 区域 | 规格 |
|------|------|
| **整体布局** | 3 列：侧边栏(240px) + 主内容(弹性, 滚动) + 右侧面板(320px, `≥xl` 显示) |
| **页面标题** | `font-headline-lg` "System Overview" + `font-body-sm` 副标题 |
| **状态徽章** | 药丸形，`rounded-full`，带脉冲 `secondary` 圆点 + "All Systems Operational" |
| **KPI 卡片** | 3 列网格（`md`+），`gap-md`；使用 KPICard 组件 |
| **流量图表** | 卡片容器，`rounded-xl p-lg`；24 柱条状图（h=30%-95%），primary/secondary 色 |
| **端点排名表** | 卡片容器；12 列网格（Method 2/12, Path 6/12, Latency 2/12, RPM 2/12）；无斑马条纹 |
| **快速操作** | `label-caps` 标题 + 3 个按钮（主/次要/次要）；`gap-sm` |
| **活动动态** | 时间线样式（`before:` 伪元素竖线）；彩色圆点指示类型 (primary/error/outline) |
| **底部间距** | `h-24 md:h-8`（为移动端导航留空间） |

---

### 2. Request Builder（请求构建器）

```
┌────────┬──────────────────────────────────┬──────────────┐
│        │ [GET ▾] https://api...    [Send] │  Response    │
│ 集合栏 │ ─────────────────────────────── │  ● 201       │
│        │ Params│Headers│Body│Auth│Scripts │  142ms 842B │
│        │ ┌────────────────────────────┐  │              │
│ 240px  │ │ ○ none ○ form ● raw [JSON▾]│  │ Body│Hdrs│Ck│
│(可折叠)│ ├────────────────────────────┤  │ [Format][Cp] │
│        │ │  1  {                      │  │  1  {        │
│ Auth   │ │  2    "username": "jdoe",  │  │  2    "id":  │
│  ├ GET │ │  3    "email": "j@e.com"   │  │  3    ...    │
│ Users  │ │  4  }                      │  │  4  }        │
│  ├ PUT │ │                            │  │              │
│        │ │                            │  │              │
└────────┴──────────────────────────────────┴──────────────┘
```

| 区域 | 规格 |
|------|------|
| **整体布局** | 3 列：集合侧栏(240px, 可折叠) + 请求编辑器(弹性) + 响应面板(400px) |
| **URL 地址栏** | 置于请求编辑器顶部；54px 行高（含 40px AddressBar + 上下间距）；AddressBar 组件 |
| **工作区标签** | 6 个标签：Params, Headers, Body, Auth, Realtime, Scripts；TabBar 组件，高度 34px |
| **Body 类型栏** | 紧贴标签栏下方，`surface-container-lowest` 背景；radio 切换 none/form-data/raw/graphql/binary；raw 模式显示 JSON/Text/XML 子类型按钮 |
| **代码编辑器** | 最小高度 118px；弹性填充剩余空间；CodeEditor 组件（编辑模式） |
| **集合侧栏** | 用户区 + "Collections" 标题 + 文件夹树（Auth/Users/Billing）+ "History" 底部区域 |
| **响应面板** | 状态栏（status badge）+ 元数据行（Time/Size）+ 标签页（Pretty/Raw/Headers/Cookies + Copy/Format 按钮）+ 空状态轻量文本；有响应后使用 CodeEditor（预览模式） |

### 请求编辑器标签页内容

| 标签 | 内容 | 使用组件 |
|------|------|----------|
| Params | Query 参数，KeyValueTable (key=128px, value=flex) | KeyValueTable |
| Headers | 请求头，KeyValueTable + Copy + Presets (+/bulk paste) | KeyValueTable |
| Body | Body 类型选择 + CodeEditor / Form 编辑器 | CodeEditor, KeyValueTable |
| Auth | 认证类型选择（None/Bearer/OAuth/Basic/JWT/API）+ 对应输入 | Dropdown + TextInput |
| Realtime | WebSocket Open/Send/End、SSE Once/Stream/Stop、gRPC draft | TextInput, CodeEditor, ActionButton |
| Scripts | Pre-request 操作行 + Tests 断言表 | KeyValueTable (Tests 模式) |

---

### 3. Mock Manager（Mock 管理器）

```
┌────────┬──────────────────────────────────┬──────────┐
│        │ GET /api/users                  │  Rules   │
│ 端点   │ ● Mock Server Status [Running]   │          │
│ 列表   │                                  │ If Head  │
│        │ Status Code: [200 ▾]            │ → resp.  │
│ 288px  │ Delay: [0ms ▾]                  │          │
│        │ Response Body: [Schema ▾]       │ ───────  │
│ ● GET  │                                  │          │
│   users│ ── Response Body ── Headers(2)── │ Live     │
│ ○ POST │  Copy →                          │ Traffic  │
│   users│  1  {                            │          │
│ ○ PUT  │  2    "status": "success",       │ 14:22 200│
│   user │  3    "data": { ... }            │ GET /api │
│ ● DEL  │  4  }                            │ 14:18 401│
│   user │                                  │ GET /api │
│        │                                  │          │
└────────┴──────────────────────────────────┴──────────┘
```

| 区域 | 规格 |
|------|------|
| **整体布局** | 3 列：端点列表(288px) + 配置面板(弹性) + 右侧（响应规则 + 流量日志，垂直分割） |
| **端点列表** | `label-caps` "ENDPOINTS" + 添加按钮；每行含状态圆点 + MethodChip + 路径；激活项 `border-l-2 border-primary` |
| **配置头** | 路径 + MethodChip + Copy 按钮 + "Mock Server Status" + Running 状态按钮 |
| **配置项** | 3 个下拉：状态码选择器、延迟选择器、响应体来源选择器 |
| **响应体预览** | 未选择端点时使用轻量空状态文本；选中端点后使用 CodeEditor 预览生成响应 |
| **响应规则** | "Routing Rules" 区域；条件卡片（If Header / If Query）+ 条件表达式 + 结果响应文件路径 |
| **流量日志** | `surface-container-lowest` 背景；每行：时间戳 + status（200=`secondary`, 401=`error`）+ 路径；悬停显示延迟/大小 |

---

### 4. Environments（环境变量）

```
┌────────┬──────────────────────────────────┬──────────┐
│        │ Staging Variables   [Filter] [+] │ Variable │
│ 环境   │ ─────────────────────────────── │ Details  │
│ 列表   │ Variable│Initial │Current │Scope │          │
│        │ DB_HOST │localhost│stg.db  │Env  │ {        │
│ 256px  │ API_KEY │••••••••│••••••••│Glb  │  "DB_... │
│        │ PORT    │3000    │8080    │Env  │ }        │
│ ● Stg  │ TIMEOUT │30      │60      │Env  │          │
│ ○ Prod │         │        │        │     │ Secret   │
│ ○ Local│         │        │        │     │ Masking  │
│        │                                  │          │
│        │                                  │ Pro Tip: │
│        │                                  │ {{A}}:{B}│
└────────┴──────────────────────────────────┴──────────┘
```

| 区域 | 规格 |
|------|------|
| **整体布局** | 3 列：环境列表(256px) + 变量表格(弹性) + 详情面板(288px) |
| **环境列表** | "Environments" 标题 + 添加按钮 + 环境项列表（绿色圆点=激活, 红色=生产, 灰色=本地） |
| **变量表格** | "Staging Variables" 标题 + 搜索 + 添加按钮；4 列：Variable/Initial Value/Current Value/Scope；Secret 值显示 `••••••••`；Scope 徽章（Env/Glb）；可编辑行 |
| **详情面板** | JSON 预览（`surface-dim` 背景，Copy 悬停按钮）+ "Secret Masking" 说明 + Pro Tip（变量引用语法 `{{DB_HOST}}:{{DB_PORT}}`） |

---

### 5. API Documentation（API 文档）

```
┌────────┬──────────────────────────────────┬──────────┐
│ 文档   │ GET /api/v1/users               │  Try It  │
│ 侧栏   │ Retrieve a list of users         │          │
│        │ [GET] /v1/users                  │ Bearer   │
│ 240px  │                                  │ [______] │
│        │ Path Parameters                  │          │
│ Search │ ┌────────┬──────┬────┬───────┐  │ user_id  │
│ [___]  │ │Name    │Type  │Req │Desc   │  │ [______] │
│        │ │page    │int   │No  │Page # │  │          │
│ Intro  │ │limit   │int   │No  │Items  │  │ [Execute]│
│ Auth   │ └────────┴──────┴────┴───────┘  │          │
│ Errors │                                  │          │
│        │ Response (200)                   │          │
│ Users  │ [200][201][400][401][404]        │          │
│  ├ GET │ ┌──────────────────────────┐    │          │
│  ├ POST│ │ { "users": [...],        │    │          │
│  ├ PUT │ │   "pagination": {...} }  │    │          │
│  └ DEL │ └──────────────────────────┘    │          │
│ Proj.▸ │                                  │          │
└────────┴──────────────────────────────────┴──────────┘
```

| 区域 | 规格 |
|------|------|
| **整体布局** | 2 列 + 侧面板：文档侧栏(240px) + 端点详情(弹性, 最大宽度 3xl) + Try It 面板(自适应) |
| **文档侧栏** | 搜索输入 + 可折叠章节（Getting Started, Users, Projects）；激活页 `border-l-2 border-primary` |
| **端点标题** | `headline-lg` 标题 + `body-lg` 描述 + MethodChip + 路径（参数高亮 `tertiary` 色） |
| **参数表** | DataTable 组件；`label-caps` 表头（Name/Type/Required/Description）；REQUIRED 徽章 10px error 色 |
| **响应区** | 状态码标签页 + JSON Schema/Example；`surface-container-lowest` 背景 + JetBrains Mono |
| **Try It 面板** | Bearer Token 输入 + 路径/Query 参数输入 + Execute 按钮（全宽, `inverse-primary` 背景 + 发光阴影） |

---

### 6. Test Runner（测试运行器）

```
┌────────┬──────────────────────────────────┬──────────┐
│ 测试   │ POST /auth/login                 │ Results  │
│ 集合   │                                  │          │
│        │ Pre-request │ Test Script        │ Status   │
│ 256px  │ ┌──────────────────────────┐    │ ● 200    │
│        │ │ 1  pm.test("Status 200") │    │ 142ms    │
│ [Run]  │ │ 2  pm.expect(...)        │    │ 1.24KB   │
│        │ │ 3                        │    │          │
│ Auth   │ └──────────────────────────┘    │ ✅ Status│
│  ├Login│                                  │ ✅ Body  │
│  ├Regis│                                  │ ✅ JSON  │
│  ├Refre│                                  │ ✅ Header│
│ Users  │                                  │          │
│  ├GET  │                                  │ Perf:    │
│  └PUT  │                                  │ ████░░░░ │
└────────┴──────────────────────────────────┴──────────┘
```

| 区域 | 规格 |
|------|------|
| **整体布局** | 3 列：测试集合(256px) + 运行器/脚本编辑器(弹性) + 结果面板(弹性) |
| **集合面板** | "Test Collections" + 搜索 + 文件夹树 + Run 按钮（全宽, `primary-container` 背景） |
| **脚本编辑** | 标签页切换（Pre-request / Test Script）+ CodeEditor（编辑模式, `surface-container-lowest` 背景） |
| **结果面板** | 响应摘要（status badge + Time + Size）+ 断言列表（`check_circle` 绿色图标）+ 性能条（DNS=`secondary` + TCP=`primary` + DL=`tertiary` 分段） |

---

### 7. Project Settings（项目设置）

```
┌────────┬──────────────────────────────┬──────────┐
│        │ General Settings             │ Audit    │
│        │                              │ Trail    │
│ 侧边栏 │ Project Metadata             │          │
│        │ Name: [______________]       │ ● Update │
│ 240px  │ Desc: [______________]       │   v2.4.0 │
│        │                              │   2m ago │
│        │ Visibility & Access          │ ● Schema │
│        │ ○ Public  ● Private          │   field   │
│        │                              │   1h ago │
│        │ ⚠ Danger Zone                │ ● Webhook│
│        │ [Transfer Ownership] [Delete]│   3h ago │
│        │                              │          │
│        │ [Cancel]  [Save Changes]     │          │
└────────┴──────────────────────────────┴──────────┘
```

| 区域 | 规格 |
|------|------|
| **整体布局** | 2 列：表单(弹性, 最大 3xl) + 审计轨迹(320px, `≥lg` 显示) |
| **表单** | 按区块组织：Project Metadata → Visibility → Danger Zone；每区块有 `label-caps` 标题 |
| **输入框** | `surface-dim` 背景, `outline-variant` 边框, 聚焦 `primary`；`code-sm` 字体 |
| **Radio 组** | 卡片式；选中态 `border-primary bg-primary-container/[0.05]` |
| **Danger Zone** | `bg-error-container/[0.1] rounded-lg border-error-container`；按钮使用 `border-error text-error` 或 `bg-error-container` |
| **审计轨迹** | 时间线样式；24px 图标圆 + 竖线；用户 + 操作 + 时间戳 |

---

### 8. API Keys（API 密钥管理）

```
┌────────┬──────────────────────────────┬──────────┐
│ 密钥   │ Production API Key           │ cURL     │
│ 列表   │ sk_prod_••••••••••••        │ ┌──────┐ │
│        │ [Copy] [Regenerate]          │ │curl..│ │
│ 25%    │                              │ └──────┘ │
│        │ Created: 2023-10-15         │          │
│ Search │ Expiry: ⚠ 3 days            │ Node.js  │
│ [___]  │ Last:    2023-10-27         │ ┌──────┐ │
│        │ Env: Production              │ │axios │ │
│ Prod.▸ │                              │ └──────┘ │
│ Staging│ [Revoke]  [Rotate]           │          │
│        │                              │          │
│        │ Usage ▂▃▅▇█                  │          │
└────────┴──────────────────────────────┴──────────┘
```

| 区域 | 规格 |
|------|------|
| **整体布局** | 2 列 + 集成面板：密钥列表(25%) + 密钥详情(弹性) + 集成面板(自适应) |
| **列表** | "Active Keys" + 搜索；每个密钥项含名称/前缀/日期；激活项 `border-l-2 border-secondary` |
| **详情** | 名称 + 掩码值 + Copy/Regenerate + 元数据网格(Created/Expiry/Last Used/Env) |
| **过期警告** | `bg-error-container/20 text-error` 徽章 |
| **操作按钮** | Revoke（`border-error text-error`）+ Rotate（`border-outline-variant`） |
| **集成面板** | cURL/Node.js 代码片段卡片（`surface` 背景 + header 含语言标签 + Copy 按钮 + code block） |

---

## 面板分割响应式规则

| 断点 | 行为 |
|------|------|
| `<md` (768px-) | 侧边栏隐藏 → 移动端底部导航；3 列布局变为单列堆叠 |
| `md-lg` (768-1280px) | 侧边栏显示(240px)；右侧面板隐藏（如 Dashboard 活动动态、Settings 审计） |
| `≥xl` (1280px+) | 全部面板显示；3 列完整布局 |

### 面板最小宽度

| 面板 | 最小宽度 | 弹性比例 |
|------|---------|----------|
| 侧边栏 | 240px | 固定 |
| Dashboard 主内容 | 0（弹性） | flex-1 |
| Dashboard 右侧面板 | 320px | 固定（`≥xl` 显示） |
| Request 编辑器 | ~300px | flex-1 |
| Response 面板 | 300px | 400px（默认） |
| Mock 端点列表 | 200px | 288px（默认） |
| Mock 流量日志 | 200px | flex-1（弹性高度） |
| Settings 表单 | 400px | flex-1, max-3xl |

---

## 页面切换动画

- 侧边栏导航点击 → 只切换常驻页面的 `visible` 状态，避免销毁并重建页面树
- 页面级 ScrollView 不使用渲染缓存层，避免首次进入复杂页面时生成缓存纹理造成卡顿
- 不实现页面间滑动动画
- 移动端底部导航切换同逻辑

## 导航层级

```
App Window
├── 顶部栏（全局）
│   ├── 菜单按钮
│   ├── API Architect 标题
│   └── 搜索按钮
├── 侧边栏（全局，md+）
│   ├── 用户区
│   ├── 项目切换器
│   ├── 主导航: Dashboard → Requests → Mocks → Docs
│   ├── 分隔线
│   ├── 次级导航: Settings → Environments → Team → API Keys
│   └── 版本号
├── 主内容区（页面切换）
│   ├── Dashboard Page
│   ├── Request Builder Page
│   ├── Mock Manager Page
│   ├── API Docs Page
│   ├── Environments Page
│   ├── Project Settings Page
│   ├── Team Management Page
│   ├── API Keys Page
│   └── Test Runner Page
├── 底部状态栏（全局）
└── 移动端底部导航（<md）
    └── Dashboard | Requests | Mocks | Docs
```

## 主内容区统一约束

所有页面的主内容区遵守：
- **内边距**: `p-gutter` (20px)
- **垂直滚动**: 独立于侧边栏
- **背景**: `background` (`#13131b`)
- **最小高度**: 填充除顶部栏和底部状态栏外的所有空间
- **底部安全区**: `pb-24 md:pb-8`（移动端导航避让）
