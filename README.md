# Video LUT Processor

一个基于Tauri框架的桌面应用程序，用于批量为视频文件应用LUT（Look-Up Table）色彩校正。底层使用FFmpeg进行视频处理，提供高效、用户友好的批量视频处理解决方案。

## 项目概述

### 核心功能
- 🎬 **批量视频处理**: 支持同时处理多个视频文件
- 🎨 **LUT色彩校正**: 支持多种LUT格式(.cube, .3dl, .lut等)
- ⚡ **高性能处理**: 基于FFmpeg的专业视频处理引擎
- 📊 **实时进度监控**: 详细的处理进度和状态反馈
- 🖥️ **跨平台支持**: Windows、macOS、Linux全平台支持
- 🎯 **用户友好界面**: 现代化的React界面设计

### 技术特点
- **前端**: React 18 + TypeScript + Tailwind CSS
- **后端**: Rust + Tauri框架
- **视频处理**: FFmpeg引擎
- **数据存储**: SQLite + 文件系统
- **状态管理**: Zustand
- **构建工具**: Vite

## 项目结构

```
auto-apply-lut/
├── 📋 技术方案.md           # 详细技术架构设计
├── 🏗️ 项目结构设计.md       # 完整项目目录结构
├── 🔌 API设计文档.md        # 前后端通信接口定义
├── 🗄️ 数据库设计.md         # 数据存储方案设计
├── 📝 开发规范.md           # 代码规范和开发流程
├── 📖 README.md            # 项目说明文档（本文件）
├── 📄 LICENSE              # 开源许可证
└── 🚫 .gitignore           # Git忽略文件配置
```

## 设计文档说明

### 📋 [技术方案.md](./技术方案.md)
详细的技术架构设计文档，包含：
- 整体架构设计
- 技术栈选择和理由
- 核心功能模块设计
- 用户界面设计
- 数据流设计
- 性能优化策略
- 错误处理机制
- 部署和分发方案
- 开发计划和风险评估

### 🏗️ [项目结构设计.md](./项目结构设计.md)
完整的项目目录结构规划，包含：
- 详细的目录树结构
- 前端React组件组织
- 后端Rust模块划分
- 配置文件说明
- 开发工作流程
- 代码规范要求

### 🔌 [API设计文档.md](./API设计文档.md)
前后端通信接口完整定义，包含：
- 数据类型定义
- 文件管理API
- LUT管理API
- 处理引擎API
- 系统配置API
- 事件系统设计
- 错误处理规范
- 完整使用示例

### 🗄️ [数据库设计.md](./数据库设计.md)
数据存储方案详细设计，包含：
- 技术选型说明
- 完整数据库表结构
- 数据访问层设计
- 性能优化策略
- 数据迁移方案
- 备份和恢复机制

### 📝 [开发规范.md](./开发规范.md)
代码规范和开发流程标准，包含：
- Rust代码规范
- TypeScript/React代码规范
- Git工作流程
- 代码审查流程
- 测试规范
- 质量保证标准
- 文档规范

## 快速开始

### 环境要求

- **Node.js**: >= 18.0.0
- **Rust**: >= 1.70.0
- **FFmpeg**: >= 4.0.0
- **操作系统**: Windows 10+, macOS 10.15+, Linux (Ubuntu 20.04+)

### 安装依赖

```bash
# 克隆项目
git clone https://github.com/your-username/auto-apply-lut.git
cd auto-apply-lut

# 安装前端依赖
npm install

# 安装Rust工具链（如果未安装）
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 安装Tauri CLI
npm install -g @tauri-apps/cli
```

### 开发模式

```bash
# 启动开发服务器
npm run tauri:dev
```

### 构建应用

```bash
# 构建生产版本
npm run tauri:build
```

## 功能特性

### 🎬 视频处理
- 支持主流视频格式：MP4, MOV, AVI, MKV等
- 批量处理多个视频文件
- 保持原始视频质量和属性
- 自定义输出格式和质量设置

### 🎨 LUT管理
- 支持多种LUT格式：.cube, .3dl, .lut, .mga
- LUT预览和强度调节
- LUT库管理和分类
- 自定义LUT参数设置

### ⚡ 性能优化
- 多线程并发处理
- 智能任务调度
- 内存使用优化
- 进度实时监控

### 🖥️ 用户界面
- 现代化Material Design风格
- 响应式布局设计
- 拖拽文件支持
- 深色/浅色主题切换

## 开发计划

### Phase 1: 基础框架 (2周)
- [x] 项目架构设计
- [x] 技术方案制定
- [ ] Tauri项目初始化
- [ ] 基础UI框架搭建
- [ ] 文件选择功能

### Phase 2: 核心功能 (3周)
- [ ] FFmpeg集成
- [ ] LUT处理逻辑
- [ ] 基础批量处理
- [ ] 数据库集成

### Phase 3: 高级功能 (2周)
- [ ] 进度监控系统
- [ ] 错误处理机制
- [ ] 性能优化
- [ ] 用户设置管理

### Phase 4: 完善和测试 (1周)
- [ ] UI优化和完善
- [ ] 跨平台测试
- [ ] 性能测试
- [ ] 用户文档编写

## 贡献指南

我们欢迎所有形式的贡献！请阅读 [开发规范.md](./开发规范.md) 了解详细的贡献流程。

### 如何贡献

1. Fork 本项目
2. 创建功能分支 (`git checkout -b feature/AmazingFeature`)
3. 提交更改 (`git commit -m 'Add some AmazingFeature'`)
4. 推送到分支 (`git push origin feature/AmazingFeature`)
5. 创建 Pull Request

### 报告问题

如果您发现了bug或有功能建议，请在 [Issues](https://github.com/your-username/auto-apply-lut/issues) 页面创建新的issue。

## 技术支持

### 常见问题

**Q: 支持哪些视频格式？**
A: 支持所有FFmpeg支持的格式，包括MP4、MOV、AVI、MKV、WebM等主流格式。

**Q: 支持哪些LUT格式？**
A: 支持.cube、.3dl、.lut、.mga等常见LUT格式。

**Q: 如何提高处理速度？**
A: 可以在设置中调整并发任务数，建议设置为CPU核心数的1-2倍。

**Q: 处理后的视频质量如何？**
A: 默认保持原始质量，也可以自定义输出质量和格式。

### 获取帮助

- 📖 查看 [用户文档](./docs/user-guide.md)
- 🐛 报告 [Bug](https://github.com/your-username/auto-apply-lut/issues)
- 💬 参与 [讨论](https://github.com/your-username/auto-apply-lut/discussions)
- 📧 联系邮箱: support@example.com

## 许可证

本项目采用 MIT 许可证 - 查看 [LICENSE](LICENSE) 文件了解详情。

## 致谢

- [Tauri](https://tauri.app/) - 跨平台应用框架
- [FFmpeg](https://ffmpeg.org/) - 视频处理引擎
- [React](https://reactjs.org/) - 前端框架
- [Rust](https://www.rust-lang.org/) - 系统编程语言

## 更新日志

### v1.0.0 (计划中)
- 初始版本发布
- 基础视频LUT处理功能
- 跨平台支持
- 用户友好界面

---

**注意**: 本项目目前处于设计阶段，实际代码实现将按照上述技术方案逐步进行。所有设计文档已完成，可以作为开发的详细指导。