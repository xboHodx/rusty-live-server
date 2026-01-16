# rusty-live-server

高性能直播互动服务器，Rust 实现。配合 SRS 实现直播流管理、观众问答鉴权和实时聊天。

## 架构

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   API Server    │     │   Chat Server   │     │  SRS Callback   │
│   :3484         │     │   :3614         │     │   :8848         │
└────────┬────────┘     └────────┬────────┘     └────────┬────────┘
         │                       │                       │
         └───────────────────────┼───────────────────────┘
                                 │
                    ┌────────────┴────────────┐
                    │       AppState          │
                    │  ┌─────────────────┐    │
                    │  │  SrsDatabase    │────┼──→ 客户端/主播状态
                    │  │  ChatDatabase   │────┼──→ 聊天消息
                    │  │  BannerDatabase │────┼──→ 问答题库
                    │  └─────────────────┘    │
                    └─────────────────────────┘
```

## 快速开始

```bash
# 编译
cargo build --release

# 创建必要目录
mkdir -p config secrets dumps

# 设置主播密钥（以 secret_ 开头）
将密钥写入 `secrets/secret.txt`，可以有多个密钥，用空格隔开

# 准备问答题库（JSON 格式）
# 见 docs/DEPLOYMENT.md 了解题库格式

# 运行
./target/release/live-server
```

## 端口

| 端口 | 服务 | 说明 |
|------|------|------|
| 3484 | API | 观众入口，处理问答鉴权 |
| 3614 | Chat | 聊天室，消息收发 |
| 8848 | SRS Callback | 接收 SRS 推拉流回调 |

## 工作流程

1. **主播推流** → SRS 调用 `:8848` 验证密钥
2. **观众请求** → `:3484` 返回问答题目
3. **答对后** → 获取播放地址，可访问 `:3614` 聊天

## 文档

- [API 接口文档](docs/API.md)
- [架构设计](docs/ARCHITECTURE.md)
- [部署运维](docs/DEPLOYMENT.md)
- [开发指南](docs/DEVELOPMENT.md)

## 技术栈

- **Axum** - Web 框架
- **Tokio** - 异步运行时
- **parking_lot** - 高性能锁
- **Serde** - 序列化

## 许可证

MIT
