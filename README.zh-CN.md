# ZenAPI

基于 Rust 和 Slint 构建的本地优先 API 工作站，将 API 测试客户端和本地 Mock
服务器整合为一个原生可执行文件。

[文档](docs/) · [设计笔记](docs/02_DESIGN.md) · [开发路线图](docs/05_TODO.md)

## 特性

- **OpenAPI / Swagger 导入** — 加载本地 JSON 或 YAML 规格文件，解析路由并构建
  交互式 API 树。
- **HTTP 客户端** — 通过 `reqwest` 发送请求，完整支持 Method、Headers、Query
  Params、Body 和 Authorization。
- **响应查看器** — 格式化 JSON、原始文本、响应头和状态码。
- **本地 Mock 服务器** — 一键启动 Axum 服务器，默认开启 CORS，基于 Schema 生成
  JSON 响应，非常适合前端开发。
- **环境与变量** — 全局和按环境的变量管理，支持 `{{name}}` 语法在 URL、Headers
  和 Body 中替换。
- **集合系统** — 将请求组织为集合 → 文件夹 → 请求三级结构，支持 Postman
  Collection v2.1 JSON 导入/导出，可从侧栏保存当前请求，使用右键菜单管理条目，
  并通过拖拽移动条目。
- **请求历史** — 本地自动记录历史，支持搜索和一键恢复。
- **代码生成** — 从任意请求生成 cURL、Python、JavaScript、Rust 和 Go 代码片段。
- **Rust + Slint 原生桌面** — 使用 Slint UI 框架，采用暗色 "Geek Modernity"
  设计系统。

## 开始使用

### 前置依赖

- [Rust](https://rustup.rs/)（stable，1.80+）
- Linux: `cmake`、`pkg-config`、`libfontconfig-dev`、`libxkbcommon-dev`、
  `libwayland-dev`（Wayland）、`libx11-dev`（X11）

### 构建与运行

```bash
git clone https://github.com/your-org/ZenAPI.git
cd ZenAPI
cargo run
```

应用窗口打开后，点击 **Import** 加载 OpenAPI 文件，从侧边栏选择路由，
然后发送你的第一个请求。Mock 服务器默认运行在 `http://127.0.0.1:8080`。

## 项目结构

```
ZenAPI/
├── ui/                         # Slint .slint UI 文件
│   ├── app.slint               # 应用外壳和主布局
│   ├── theme.slint             # 全局颜色/间距/字体 token
│   └── widgets/                # 计划拆分的可复用 UI 组件
├── src/
│   ├── main.rs                 # Slint 应用入口
│   ├── lib.rs                  # 库根文件
│   ├── app.rs                  # Slint 状态、动作和工作流绑定
│   ├── openapi.rs              # OpenAPI 模块入口
│   ├── openapi/model.rs        # 解析后的路由和 Schema 模型
│   ├── openapi/parser.rs       # OpenAPI 3.0 / Swagger 2.0 文件解析器
│   ├── openapi/json.rs         # JSON 格式处理
│   ├── openapi/yaml.rs         # YAML 格式处理
│   ├── openapi/schema.rs       # Schema → Mock 数据生成
│   ├── client.rs               # HTTP 客户端模块入口
│   ├── client/transport.rs     # reqwest 请求传输层
│   ├── client/response.rs      # 响应格式化
│   ├── mock_server.rs          # Mock 服务器模块入口
│   ├── mock_server/server.rs   # Axum 服务器生命周期
│   ├── mock_server/routing.rs  # 动态 Mock 路由生成
│   ├── collections.rs          # 集合树和 Postman 导入/导出
│   ├── variables.rs            # 变量存储与插值替换
│   ├── history.rs              # 请求历史模型与过滤
│   └── codegen.rs              # 多语言代码片段生成
├── docs/
│   ├── 01_PRD.md               # 产品需求与 MVP 范围
│   ├── 02_DESIGN.md            # 视觉与交互设计决策
│   ├── 05_TODO.md              # Slint 迁移路线图与任务追踪
│   └── 07_USER_GUIDE.md        # 用户指南
├── stitch_nextgen_api_studio/  # 设计参考（Nexus API 设计系统）
├── Cargo.toml
├── Cargo.lock
└── build.rs                    # slint-build 编译
```

### 核心依赖

| Crate | 用途 |
|-------|------|
| `slint` / `slint-build` | 声明式桌面 UI，编译时 `.slint` 处理 |
| `reqwest` | HTTP/HTTPS 客户端，TLS 支持 |
| `axum` / `tokio` | 本地 Mock 服务器（异步，默认 CORS） |
| `serde_json` / `serde_yaml` | OpenAPI 文档解析 |

## 设计系统

ZenAPI 遵循 **Nexus API 设计系统** —— 一种暗色 "Geek Modernity" 美学，
定义于 `stitch_nextgen_api_studio/nexus_api/DESIGN.md`。关键设计 token：

- **背景色**: 深炭灰 `#13131b`
- **主色**: Vibrant Indigo `#c0c1ff`
- **次要色**: Cyber Mint `#4edea3`（成功状态、活跃端点）
- **字体**: Inter（UI）+ JetBrains Mono（代码）
- **图标**: Material Symbols Outlined
- **布局**: 12 列流式网格，240px 可折叠侧边栏

详见 [docs/02_DESIGN.md](docs/02_DESIGN.md) 获取完整实现指南。

## 文档

- [PRD](docs/01_PRD.md) — 产品需求与 MVP 范围
- [DESIGN](docs/02_DESIGN.md) — 视觉与交互指南
- [TODO](docs/05_TODO.md) — 开发路线图
- [User Guide](docs/07_USER_GUIDE.md)

## 平台支持

| 平台 | 状态 |
|------|------|
| Linux (Wayland) | ✅ 主要开发平台 |
| Linux (X11) | ✅ 支持 |
| macOS | 计划中 |
| Windows | 计划中 |

## 许可证

除另有说明外，ZenAPI 源代码以 MIT License 或 Apache License 2.0 双许可（任选其一）。
