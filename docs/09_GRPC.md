# gRPC 支持评估

> 状态: 评估完成，domain draft/catalog validation 已接入 Slint，FileDescriptorSet/proto source 解析和加载已接入 Slint，reflection descriptor 加载已接入 domain
> 日期: 2026-06-13

## 目标

ZenAPI 的 gRPC 工作区应覆盖常见调试流：

- 读取 `.proto` 或 server reflection 服务定义
- 展示 package / service / method 树
- 支持 unary 调用
- 后续支持 server streaming / client streaming / bidirectional streaming
- 使用 JSON 形式编辑 protobuf message
- 展示 response metadata、status code、trailers 和 message 流

## 推荐技术路线

优先采用 Rust 原生生态：

- `tonic = "0.14"`: gRPC over HTTP/2 客户端基础
- `prost = "0.14"`: protobuf 编解码基础
- `prost-reflect = "0.16"`: 动态 message、descriptor pool、JSON-like 编辑映射
- `tonic-reflection = "0.14"`: server reflection 查询

依赖版本保持主/次版本范围，不锁补丁号，符合当前依赖升级策略。

## 不建议的路线

- 不在第一版内 shell out 到 `grpcurl`：会增加外部二进制依赖、跨平台打包和错误解析复杂度。
- 不把 `.proto` 编译放进 ZenAPI 主 crate 的 `build.rs`：用户导入的 proto 是运行时数据，不应变成应用构建时依赖。
- 不先实现 streaming 再实现 unary：streaming UI 和生命周期复杂度更高，应该建立在 unary 元数据、descriptor 和 message 编辑都稳定之后。

## 分阶段实现

### 1. Descriptor 加载

输入：

- `.proto` 文件路径
- include path 列表
- reflection endpoint

输出：

- package / service / method 列表
- method 类型: unary、server streaming、client streaming、bidi streaming
- request / response message descriptor

实现要点：

- `.proto` 路径先通过 `prost-reflect` 的 descriptor pool 工作流承载。
- 如果需要调用 `protoc`，只作为可配置工具路径，不把特定二进制写死进仓库。
- reflection 路径通过 `tonic-reflection` 获取 `FileDescriptorSet`。

### 2. Unary 调用 MVP

UI：

- gRPC endpoint 输入，例如 `http://localhost:50051`
- service/method 选择
- metadata 键值编辑
- request JSON 编辑区
- Invoke 按钮
- response message / metadata / trailers / status 展示

执行：

- JSON 输入转动态 protobuf message
- 使用 `tonic` 发 unary 请求
- response message 转 JSON 预览
- 错误展示 gRPC status code、message、details

### 3. Streaming

后续在 unary 稳定后做：

- server streaming: 订阅响应流，支持 Stop
- client streaming: 多条 request message 队列，支持 Send / Finish
- bidi streaming: 持久连接、双向 message log，与 WebSocket 面板的会话模型保持一致

## UI 集成建议

gRPC 不应塞进 HTTP Body 面板。推荐单独面板或协议工作区：

- 左侧保持 OpenAPI / Collection 树
- 请求侧新增 gRPC 区块：endpoint、method、metadata、message editor
- 响应侧复用现有 response viewer 的状态、headers-like metadata 和只读文本展示

与现有功能复用：

- 变量系统: endpoint、metadata、message JSON 支持 `{{var}}`
- History: 记录 method path、status、message preview
- Collection: 增加协议字段时保持 serde 默认值，保证旧 HTTP collection 可读
- Runner: 后续可将 unary gRPC 请求接入 collection runner

## 风险

- protobuf import 路径解析复杂，尤其是多 include root。
- 动态 protobuf JSON 映射需要清晰错误提示，否则用户很难定位字段类型问题。
- TLS、authority、compression、deadline、metadata 二进制 header 会增加配置面。
- streaming UI 生命周期复杂，必须避免阻塞现有 HTTP、WebSocket、SSE 操作。

## 建议 TODO 拆分

1. [x] 建立 gRPC domain model 并接入 Slint draft：endpoint、metadata、method descriptor catalog、message JSON。
2. [x] 实现 `FileDescriptorSet` / `.protoset` method catalog 提取测试。
3. [x] 接入 Slint `.protoset` 加载到 gRPC method catalog。
4. [x] 实现 reflection descriptor 加载测试。
5. [x] 实现 `.proto` 源文件 descriptor 加载测试。
6. [x] 接入 Slint `.proto` 源文件加载到 gRPC method catalog。
7. [ ] 实现 unary 调用传输层和本地 tonic 服务测试。
8. [ ] 接入 Slint unary 面板。
9. [ ] 再实现 server streaming。

## 当前结论

ZenAPI 可以采用 `tonic` + `prost-reflect` + `tonic-reflection` 的原生实现路线。第一版只做 descriptor 加载和 unary 调用，streaming 延后到协议工作区状态模型稳定后实现。
