# 设置弹窗系统诊断接入设计

> 状态：已实现并完成前端接入验证

## 背景

当前后端已经实现并注册了一组系统诊断相关 Tauri 命令：

- `get_system_info`
- `get_log_files`
- `read_log_file`
- `clear_cache`
- `get_cache_size`

这些能力现在已经在前端 [SettingsModal.tsx](file:///Users/dafuchen/Develop/video/auto-apply-lut/src/components/SettingsModal.tsx) 中接通，用户可以直接在 `处理设置` 弹窗中查看系统信息、缓存状态和应用日志。

现有产品里最自然的承载位置是 [SettingsModal.tsx](file:///Users/dafuchen/Develop/video/auto-apply-lut/src/components/SettingsModal.tsx)，因为它已经包含：

- 编解码器探测
- GPU 诊断
- 硬件加速测试

也就是说，`处理设置` 弹窗已经承担一部分“配置 + 诊断”职责。本次实现保持这一结构，在不新增独立页面的前提下，把系统信息、缓存信息、日志查看补进了该弹窗。

## 实现结果

本次设计对应能力已完成落地，当前行为如下：

- 弹窗打开时自动请求 `get_system_info` 和 `get_cache_size`
- 用户可以手动刷新系统与缓存信息
- 用户可以执行 `clear_cache` 并在成功后自动回刷缓存大小
- 用户可以点击 `加载日志` 获取日志文件列表
- 用户可以选择日志文件并读取只读日志内容
- 浏览器预览模式下会显示明确降级提示，不会因 Tauri 不可用而崩溃
- 单项失败不会影响其它设置区块和诊断区块的展示

## 目标

- 在 `处理设置` 弹窗中新增系统诊断入口，不新增独立页面
- 让前端真实调用以下 5 个命令：
  - `get_system_info`
  - `get_log_files`
  - `read_log_file`
  - `clear_cache`
  - `get_cache_size`
- 为用户提供可读的系统摘要、缓存大小、日志文件列表和日志内容预览
- 保持当前浏览器预览模式下的优雅降级，不因 Tauri 不可用而导致弹窗崩溃
- 尽量复用 `SettingsModal` 现有布局与交互模式，避免引入新的复杂导航

## 非目标

- 不在本次接入文件管理 5 个命令：
  - `list_directory`
  - `create_directory`
  - `delete_path`
  - `copy_file`
  - `move_file`
- 不新增独立“系统中心”或“开发者工具”页面
- 不支持日志编辑、下载、分享、搜索、高亮
- 不支持自动轮询日志或实时流式追踪
- 不支持在前端直接删除日志文件
- 不调整现有视频处理主流程与设置保存逻辑

## 方案选择

本次采用 `方案A`：

- 只把系统信息、缓存管理、日志查看接入 `SettingsModal`
- 文件管理命令保留为后续能力，不在这次前端接入范围内

选择原因：

- 改动范围最小，适合当前产品结构
- 与 `SettingsModal` 已有的诊断职责一致
- 能最快把“后端已实现但前端未使用”的缺口补齐
- 避免因为文件管理 UI 扩 scope，影响当前主线功能稳定性

## 用户体验

### 入口位置

- 在 `处理设置` 弹窗中新增一个 `系统与缓存` 分区
- 在同一大区内新增一个 `日志` 分区
- 两个分区均已放置在现有 `GPU 诊断` 区块之后，保持“诊断能力集中展示”

### 系统与缓存区

当前展示内容：

- 操作系统名称与版本
- CPU 核心数
- CPU 占用率
- 内存占用率
- 缓存大小

操作按钮：

- `刷新`
- `清理缓存`

当前交互规则：

- 打开弹窗时自动拉取一次 `get_system_info` 和 `get_cache_size`
- 用户点击 `刷新` 时重新拉取这两项
- 用户点击 `清理缓存` 时调用 `clear_cache`
- 清理成功后显示 `缓存已清理`，并自动刷新缓存大小
- 清理失败时显示轻量错误提示，不关闭弹窗

### 日志区

当前展示内容：

- 日志文件列表
- 当前选中文件的日志内容预览

当前交互规则：

1. 用户打开弹窗
2. 日志区默认等待用户点击 `加载日志`
3. 前端调用 `get_log_files`
4. 用户点击某个日志文件
5. 前端调用 `read_log_file`
6. 在同一区块下方展示日志文本内容

状态要求：

- 空状态：无日志文件
- 加载态：正在加载日志列表 / 正在读取日志
- 错误态：日志加载失败
- 只读态：日志文本不可编辑

## 架构设计

### 前端改动

本次主要改动文件：

- [SettingsModal.tsx](file:///Users/dafuchen/Develop/video/auto-apply-lut/src/components/SettingsModal.tsx)
- [SettingsModal.css](file:///Users/dafuchen/Develop/video/auto-apply-lut/src/components/SettingsModal.css)
- [SettingsModal.test.tsx](file:///Users/dafuchen/Develop/video/auto-apply-lut/src/components/SettingsModal.test.tsx)

前端新增职责：

- 在弹窗打开时拉取系统摘要与缓存大小
- 按用户操作延迟加载日志文件列表
- 在用户选择日志文件时读取内容
- 展示系统/缓存/日志的加载态与失败态
- 在 Tauri 不可用时给出明确降级提示

本次前端新增状态：

- `systemInfo`
- `systemInfoLoading`
- `systemInfoError`
- `cacheSize`
- `cacheLoading`
- `cacheActionLoading`
- `logFiles`
- `logFilesLoading`
- `logFilesError`
- `selectedLogFile`
- `logContent`
- `logContentLoading`
- `logContentError`
- `hasLoadedLogs`
- `cacheActionMessage`

### 后端边界

本次不新增新的 Rust 命令，只消费已经存在且已注册的接口：

- `get_system_info`
- `get_log_files`
- `read_log_file`
- `clear_cache`
- `get_cache_size`

命令本身的参数和返回结构如果与前端预期不完全一致，由前端做轻量映射，但不在这次设计中扩展后端职责。

## 数据流

### 弹窗打开

1. `SettingsModal` 打开
2. 现有逻辑继续执行：
   - `get_available_codecs`
   - GPU 相关探测
3. 新增逻辑并行执行：
   - `get_system_info`
   - `get_cache_size`
4. 前端更新系统与缓存展示区
5. 如处于浏览器预览模式，则显示明确降级提示

### 清理缓存

1. 用户点击 `清理缓存`
2. 前端调用 `clear_cache`
3. 成功后再次调用 `get_cache_size`
4. 前端刷新缓存大小并提示结果

### 加载日志列表

1. 用户点击 `加载日志`
2. 前端调用 `get_log_files`
3. 前端渲染日志文件列表
4. 当前实现保持手动选择日志文件，不自动读取第一项

### 读取日志文件

1. 用户点击某个日志文件
2. 前端调用 `read_log_file`
3. 前端展示日志文本
4. 如读取失败，保留文件列表并展示错误提示

## 降级与错误处理

继续沿用 [safeInvoke](file:///Users/dafuchen/Develop/video/auto-apply-lut/src/components/SettingsModal.tsx#L77-L94) 的运行时判断与异常捕获逻辑。

其中一个关键实现修正是：只有真正的 Tauri 不可用场景才会抛出 `tauri_unavailable`，而命令本身的业务错误会被保留下来，用于区分“预览模式降级”和“真实调用失败”。

处理原则：

- 浏览器预览模式：
  - 不抛未捕获异常
  - 展示“当前环境不支持此诊断能力”类提示
- 单项失败不影响其他区块展示
  - 例如日志失败不应影响 GPU 和基础设置
- 清理缓存失败不应清空当前缓存大小展示
- 日志读取失败不应清空日志文件列表

提示文案：

- `当前环境不支持系统诊断（浏览器预览模式）`
- `当前环境不支持日志查看（浏览器预览模式）`
- `系统信息加载失败`
- `缓存信息加载失败`
- `清理缓存失败`
- `日志列表加载失败`
- `日志内容读取失败`

## UI 细节

### 布局

- 保持 `settings-grid` 现有两列/自适应模式
- 系统与缓存区、日志区沿用 `settings-section` 视觉风格
- 日志列表和日志内容在单个 section 内做上下布局
- 在较宽屏幕下允许扩展为左右分栏；较窄时退化为上下堆叠

### 可读性

- 系统摘要以 key-value 的简洁文本展示
- 缓存大小使用人类可读单位
- 日志内容区域使用等宽字体、保留换行
- 日志预览区设置最大高度并允许内部滚动，避免弹窗整体过度变长

### 禁用态

- 当 `disabled` 为真时：
  - 禁用刷新和清理缓存按钮
  - 允许查看已加载信息
- 日志文件读取中会显示 `正在读取日志...`，避免用户误判为无响应

## 测试设计

### 前端测试

已覆盖范围：

- 弹窗打开后会请求系统信息和缓存大小
- Tauri 不可用时显示降级提示
- 点击 `清理缓存` 后会重新刷新缓存大小
- 点击日志区后加载日志文件列表
- 选择日志文件后显示日志内容
- 日志读取失败时显示错误但保留列表
- `safeInvoke` 在真实错误场景下保留原始异常语义

### 后端测试

本次原则上不新增 Rust 测试，因为当前目标不是实现新命令，而是接通前端使用链路。

如果实际检查发现某个已注册命令的返回结构与前端完全不匹配，再针对该命令补最小必要测试。

## 实施边界

本次完成标准：

- `SettingsModal` 中可以看到系统与缓存区
- 前端真实调用 `get_system_info` 和 `get_cache_size`
- 前端可以执行 `clear_cache` 并刷新展示
- 前端可以加载日志文件列表并读取某个日志文件内容
- 浏览器预览模式不崩溃，且有明确降级反馈

当前状态：以上条件均已达成，本次“后端能力接入前端”的目标已完成。
