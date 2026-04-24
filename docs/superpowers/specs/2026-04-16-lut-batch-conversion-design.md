# LUT 文件批量转换设计

## 背景

当前项目已经具备：

- LUT 文件资料库与选择能力
- `LutConverter` 的内存态格式转换能力
- 多种 LUT 格式的解析与部分写出能力

但“从文件读取 LUT -> 转换格式 -> 写回为新文件”的完整链路尚未打通。核心阻塞点是 [convert_file](file:///Users/dafuchen/Develop/video/auto-apply-lut/src-tauri/src/core/lut/converter.rs#L373-L381) 仍为 `todo!()`，因此现阶段无法提供真正可用的 LUT 文件批量转换功能。

本设计定义第一版 `LUT 文件批量转换功能`，优先服务：

1. 资料库整理
2. 视频处理前的 LUT 预处理

其中第一版只在 `LUT 资料库` 内提供入口，不扩展到主界面独立入口。

## 目标

- 允许用户对 `LUT 资料库` 中当前已选中的多个 LUT 文件执行批量格式转换
- 转换后的新文件生成在原文件旁边
- 当目标文件名冲突时，自动追加递增后缀，避免覆盖原文件
- 单文件失败不影响其余文件
- 转换结束后自动刷新资料库并展示成功/失败摘要

## 非目标

- 不提供主界面“手动选一批 LUT 直接转换”的独立入口
- 不支持跨维度转换
  - 不支持 3D -> 1D
  - 不支持 1D -> 3D
- 不支持覆盖原文件
- 不支持输出到指定目录
- 不支持后台长任务、实时进度事件流、取消转换
- 不支持复杂命名模板、批量重命名规则

## 第一版用户体验

### 入口

- 在 `LUT 资料库` 弹窗头部操作区新增 `批量转换` 按钮
- 按钮只对“当前选中的 LUT 文件”生效
- 当未选中任何 LUT 时，按钮禁用

### 交互流程

1. 用户在资料库中选中若干 LUT 文件
2. 点击 `批量转换`
3. 打开轻量转换弹窗
4. 弹窗中展示：
   - 已选文件数量
   - 可选目标格式
   - 开始转换按钮
5. 用户选择目标格式并确认
6. 前端调用批量转换命令
7. 转换完成后在弹窗中展示：
   - 成功数量
   - 失败数量
   - 每个失败文件的原因
8. 资料库自动刷新

### 输出规则

- 新文件输出到源文件同目录
- 默认文件名规则：
  - `film.cube -> film.converted.csp`
- 如目标文件已存在，则自动追加序号：
  - `film.converted.csp`
  - `film.converted-1.csp`
  - `film.converted-2.csp`

### 转换完成后的行为

- 不自动关闭结果弹窗，便于用户查看失败原因
- 不自动替换当前选中集合
- 新生成的 LUT 只刷新到资料库中，不自动加入“当前任务选中”

## 格式策略

### 第一版允许的转换

基于当前代码中已有 parser / writer / conversion map 的真实能力，第一版只开放以下组合：

- 3D 组内：
  - `.cube`
  - `.3dl`
  - `.csp`
  - `.m3d`
- 特殊兼容：
  - `.look <-> .cube`
- 1D 组内：
  - `.lut <-> .mga`

### 第一版禁止的情况

- 混合选择 1D 与 3D LUT
- 目标格式与源格式属于不同维度
- 代码尚无 writer 支持的格式输出

### 前端目标格式展示规则

前端不直接展示“全部格式”，而是根据当前选中文件动态计算“共同可转换的目标格式”：

- 如果当前选中项全为 3D：
  - 仅展示该 3D 组与特殊兼容组中对当前集合都成立的目标格式
- 如果当前选中项全为 1D：
  - 仅展示 `.lut` 和 `.mga` 中合法目标格式
- 如果当前选中项包含 1D + 3D 混合：
  - 禁止开始批量转换
  - 提示“当前选中项包含不同维度的 LUT，无法批量转换到同一目标格式”

## 架构设计

### 前端

主要改动组件：

- [LutLibraryPanel.tsx](file:///Users/dafuchen/Develop/video/auto-apply-lut/src/components/LutLibraryPanel.tsx)
- [LutLibraryPanel.css](file:///Users/dafuchen/Develop/video/auto-apply-lut/src/components/LutLibraryPanel.css)

新增前端职责：

- 根据当前选中项推导可选目标格式
- 展示批量转换弹窗
- 调用 Tauri 批量转换命令
- 渲染转换摘要与失败列表
- 成功后刷新资料库

前端新增状态建议：

- `isBatchConvertOpen`
- `batchConvertTargetFormat`
- `isBatchConverting`
- `batchConvertResult`

### 后端

主要改动模块：

- [converter.rs](file:///Users/dafuchen/Develop/video/auto-apply-lut/src-tauri/src/core/lut/converter.rs)
- [lut_manager.rs](file:///Users/dafuchen/Develop/video/auto-apply-lut/src-tauri/src/commands/lut_manager.rs)
- `src-tauri/src/types/` 下新增或扩展批量转换请求/结果类型

后端新增职责：

- 将文件路径解析为 `LutData`
- 将 `LutData` 转换为目标格式
- 将转换结果写出为新文件
- 处理文件名冲突
- 汇总逐文件结果返回前端

## 核心数据流

1. 前端收集 `selectedLutPaths`
2. 前端根据现有 metadata 判断维度与可选目标格式
3. 前端发送 `paths + target_format`
4. 后端逐文件执行：
   - 识别源格式
   - 调用对应 parser 读取为 `LutData`
   - 校验源格式与目标格式是否兼容
   - 调用 `LutConverter::convert()`
   - 计算输出文件路径
   - 调用对应 writer 写文件
5. 后端返回批量结果数组
6. 前端显示成功/失败摘要并刷新资料库

## 后端实现细节

### 1. 补全 `convert_file`

`convert_file` 负责将单个 LUT 文件从磁盘读取、转换、写回。

职责拆分：

- `load_lut_file(path) -> LutData`
  - 按扩展名选择 parser
- `convert(lut_data, target_format, options) -> LutData`
  - 复用现有转换逻辑
- `build_output_path(source_path, target_format) -> PathBuf`
  - 生成 `*.converted.*` 路径并处理冲突
- `write_lut_file(lut_data, output_path)`
  - 按目标格式选择 writer

### 2. 输出路径策略

统一由单一 helper 生成，避免命名逻辑散落在 command 层。

规则：

- 取源文件 stem
- 追加 `.converted`
- 替换为目标扩展名
- 若冲突则追加 `-1`, `-2`, `-3`

### 3. 批量命令返回结构

建议返回：

- `source_path`
- `target_path`
- `success`
- `error_message`

并在顶层额外返回：

- `success_count`
- `failure_count`

这样前端无需再次聚合。

## 错误处理

单文件级错误不应终止批处理。

建议统一错误文案：

- `无法识别 LUT 格式`
- `该 LUT 格式暂不支持导出`
- `源格式与目标格式不兼容`
- `LUT 文件解析失败`
- `目标文件写入失败`

前端结果展示规则：

- 全部成功：显示“已成功转换 X 个 LUT 文件”
- 部分成功：显示“成功 X 个，失败 Y 个”
- 全部失败：显示“本次转换未成功，请检查失败原因”

## 测试设计

### Rust

新增或扩展测试覆盖：

- `convert_file` 能够读取单个文件并成功输出
- 目标路径冲突时自动追加后缀
- 不兼容格式组合被正确拒绝
- 批量转换时单文件失败不影响其他文件
- 写出后的扩展名与目标格式一致

### Frontend

新增或扩展测试覆盖：

- 未选中 LUT 时 `批量转换` 按钮禁用
- 目标格式列表按选中 LUT 动态收缩
- 混合维度选择时禁止转换
- 转换结果摘要可见
- 部分失败时能显示逐文件错误信息

## 实施边界

第一版以“资料库整理可用”为优先目标。

判断标准：

- 用户能从资料库中选中多个 LUT
- 能发起合法的批量格式转换
- 新文件能落在原文件旁边且不覆盖原文件
- 转换结果清晰可见
- 资料库能在转换后自动刷新

若以上全部达成，即视为第一版完成。
