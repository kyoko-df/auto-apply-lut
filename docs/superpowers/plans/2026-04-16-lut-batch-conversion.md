# LUT 文件批量转换 Implementation Plan

> 状态：已完成实现与文档回填
> 说明：以下 TDD checklist 已按当前仓库落地状态回填为完成，用于反映现状，不作为历史逐步执行记录。

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** 为 `LUT 资料库` 增加可用的批量格式转换功能，支持将当前选中的 LUT 文件批量转换为兼容目标格式，并将新文件输出到原文件旁边。

**Architecture:** 以后端 `LutConverter` 为核心，先补齐 `convert_file` 的“文件读取 -> 解析 -> 转换 -> 写回”链路，再在 Tauri command 层暴露批量转换接口，最后由前端 `LutLibraryPanel` 增加转换弹窗与结果展示。第一版只服务资料库整理，不引入后台事件流或主界面独立入口。

**Tech Stack:** Rust, Tauri v2, React 19, TypeScript, Vitest, Testing Library

## 完成情况

- 已落地 `src-tauri/src/types/lut_conversion.rs`，补齐批量转换请求与结果类型
- 已完成 `LutConverter::convert_file()` 文件读取、转换、写回、命名冲突处理链路
- 已实现 `batch_convert_luts` Tauri 命令并在 `src-tauri/src/lib.rs` 注册
- 已在 `LutLibraryPanel` 中增加批量转换入口、目标格式选择与结果摘要展示
- 已补充 Rust 与前端回归测试，覆盖输出路径、冲突后缀、维度限制和结果展示

---

## 文件结构

### 现有文件

- `src-tauri/src/core/lut/converter.rs`
  - 负责 LUT 格式转换逻辑
  - 当前已有内存态 `convert()`、`batch_convert()` 与完整的 `convert_file()` 文件链路实现
- `src-tauri/src/core/lut/parser.rs`
  - 负责各格式 LUT 的 parse / write / header 逻辑
- `src-tauri/src/core/lut/mod.rs`
  - 负责 LUT 管理、验证和资料库扫描
- `src-tauri/src/commands/lut_manager.rs`
  - 负责 LUT 资料库相关 Tauri 命令
- `src-tauri/src/types/lut.rs`
  - 负责 LUT 相关基础类型定义
- `src-tauri/src/types/mod.rs`
  - 负责导出公共类型
- `src-tauri/src/lib.rs`
  - 负责 Tauri command 注册
- `src/components/LutLibraryPanel.tsx`
  - 负责 LUT 资料库 UI
- `src/components/LutLibraryPanel.css`
  - 负责 LUT 资料库样式
- `src/components/LutLibraryPanel.test.tsx`
  - 负责 LUT 资料库组件回归测试

### 新增文件

- `src-tauri/src/types/lut_conversion.rs`
  - 负责 LUT 批量转换请求 / 结果类型

### 测试文件

- `src/components/LutLibraryPanel.test.tsx`
- `src-tauri/src/core/lut/converter.rs` 内联单元测试
- `src-tauri/src/commands/lut_manager.rs` 内联单元测试或命令接口测试

---

### Task 1: 定义批量转换类型与命令接口边界

**Files:**
- Create: `src-tauri/src/types/lut_conversion.rs`
- Modify: `src-tauri/src/types/mod.rs`
- Modify: `src-tauri/src/commands/lut_manager.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/commands/lut_manager.rs`

- [x] **Step 1: 写失败测试，锁定请求/结果类型与命令签名**

在 `src-tauri/src/commands/lut_manager.rs` 的测试模块中添加一个最小编译期测试，约束新命令返回的结果结构至少包含成功/失败统计和逐文件结果：

```rust
#[cfg(test)]
mod batch_convert_contract_tests {
    use crate::types::lut_conversion::{
        BatchConvertLutsRequest,
        BatchConvertLutsResponse,
        BatchConvertLutItemResult,
    };
    use crate::types::LutFormat;

    #[test]
    fn batch_convert_response_contract_is_stable() {
        let request = BatchConvertLutsRequest {
            paths: vec!["/tmp/a.cube".to_string()],
            target_format: LutFormat::Csp,
        };

        let item = BatchConvertLutItemResult {
            source_path: "/tmp/a.cube".to_string(),
            target_path: Some("/tmp/a.converted.csp".to_string()),
            success: true,
            error_message: None,
        };

        let response = BatchConvertLutsResponse {
            success_count: 1,
            failure_count: 0,
            results: vec![item],
        };

        assert_eq!(request.paths.len(), 1);
        assert_eq!(response.success_count, 1);
        assert_eq!(response.failure_count, 0);
        assert_eq!(response.results.len(), 1);
    }
}
```

- [x] **Step 2: 运行测试，确认因缺少类型而失败**

Run:

```bash
cd /Users/dafuchen/Develop/video/auto-apply-lut/src-tauri
cargo test batch_convert_response_contract_is_stable
```

Expected:
- FAIL
- 报错提示 `crate::types::lut_conversion` 不存在，或类型未定义

- [x] **Step 3: 写最小实现，增加类型文件与类型导出**

创建 `src-tauri/src/types/lut_conversion.rs`：

```rust
use crate::types::LutFormat;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchConvertLutsRequest {
    pub paths: Vec<String>,
    pub target_format: LutFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchConvertLutItemResult {
    pub source_path: String,
    pub target_path: Option<String>,
    pub success: bool,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchConvertLutsResponse {
    pub success_count: usize,
    pub failure_count: usize,
    pub results: Vec<BatchConvertLutItemResult>,
}
```

修改 `src-tauri/src/types/mod.rs`：

```rust
pub mod lut_conversion;

pub use lut_conversion::{
    BatchConvertLutItemResult,
    BatchConvertLutsRequest,
    BatchConvertLutsResponse,
};
```

- [x] **Step 4: 运行测试，确认类型契约通过**

Run:

```bash
cd /Users/dafuchen/Develop/video/auto-apply-lut/src-tauri
cargo test batch_convert_response_contract_is_stable
```

Expected:
- PASS

- [x] **Step 5: 提交**

```bash
cd /Users/dafuchen/Develop/video/auto-apply-lut
git add src-tauri/src/types/lut_conversion.rs src-tauri/src/types/mod.rs src-tauri/src/commands/lut_manager.rs
git commit -m "feat: add lut batch conversion types"
```

---

### Task 2: 为 `convert_file` 建立单文件转换的失败测试

**Files:**
- Modify: `src-tauri/src/core/lut/converter.rs`
- Test: `src-tauri/src/core/lut/converter.rs`

- [x] **Step 1: 写失败测试，锁定单文件转换输出行为**

在 `src-tauri/src/core/lut/converter.rs` 的测试模块中新增：

```rust
#[tokio::test]
async fn test_convert_file_writes_converted_file_next_to_source() {
    use std::path::PathBuf;
    use tempfile::tempdir;
    use tokio::fs;

    let converter = LutConverter::new();
    let dir = tempdir().expect("temp dir");
    let source_path = dir.path().join("sample.cube");

    fs::write(
        &source_path,
        r#"TITLE "Sample"
LUT_3D_SIZE 2
0.0 0.0 0.0
1.0 0.0 0.0
0.0 1.0 0.0
1.0 1.0 0.0
0.0 0.0 1.0
1.0 0.0 1.0
0.0 1.0 1.0
1.0 1.0 1.0
"#,
    )
    .await
    .expect("write source lut");

    let options = ConversionOptions::default();
    let converted = converter
        .convert_file(&source_path, LutFormat::Csp, &options)
        .await
        .expect("convert file");

    let output_path = dir.path().join("sample.converted.csp");
    let written = fs::read_to_string(&output_path).await.expect("read output");

    assert_eq!(converted.format, LutFormat::Csp);
    assert!(output_path.exists());
    assert!(!written.is_empty());
}
```

- [x] **Step 2: 运行测试，确认因 `todo!()` 失败**

Run:

```bash
cd /Users/dafuchen/Develop/video/auto-apply-lut/src-tauri
cargo test test_convert_file_writes_converted_file_next_to_source
```

Expected:
- FAIL
- 报错来自 `todo!("Implement file loading and conversion")`

- [x] **Step 3: 写最小辅助实现，先补路径生成函数**

在 `src-tauri/src/core/lut/converter.rs` 中为 `LutConverter` 增加：

```rust
fn build_output_path(&self, source_path: &Path, target_format: LutFormat) -> AppResult<std::path::PathBuf> {
    let parent = source_path.parent().ok_or_else(|| {
        AppError::InvalidInput("Source path has no parent directory".to_string())
    })?;
    let stem = source_path
        .file_stem()
        .and_then(|v| v.to_str())
        .ok_or_else(|| AppError::InvalidInput("Source file name is invalid".to_string()))?;

    let extension = target_format.extension();
    let mut candidate = parent.join(format!("{stem}.converted.{extension}"));
    let mut index = 1usize;

    while candidate.exists() {
        candidate = parent.join(format!("{stem}.converted-{index}.{extension}"));
        index += 1;
    }

    Ok(candidate)
}
```

- [x] **Step 4: 写最小辅助实现，补 parser/writer 分发函数**

在 `src-tauri/src/core/lut/converter.rs` 中加入：

```rust
async fn load_lut_file(&self, file_path: &Path) -> AppResult<LutData> {
    use crate::core::lut::parser::{CubeParser, CspParser, LookParser, LutParser, M3dParser, MgaParser, ThreeDLParser};

    let format = LutFormat::from_extension(
        file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default(),
    );

    match format {
        LutFormat::Cube => CubeParser::parse(file_path).await,
        LutFormat::ThreeDL => ThreeDLParser::parse(file_path).await,
        LutFormat::Lut => CubeParser::parse(file_path).await,
        LutFormat::Csp => CspParser::parse(file_path).await,
        LutFormat::M3d => M3dParser::parse(file_path).await,
        LutFormat::Look => LookParser::parse(file_path).await,
        LutFormat::Mga => MgaParser::parse(file_path).await,
        LutFormat::Unknown => Err(AppError::Validation("无法识别 LUT 格式".to_string())),
    }
}

async fn write_lut_file(&self, lut_data: &LutData, output_path: &Path) -> AppResult<()> {
    use crate::core::lut::parser::{CubeParser, CspParser, LookParser, LutParser, M3dParser, MgaParser, ThreeDLParser};

    match lut_data.format {
        LutFormat::Cube => CubeParser::write(lut_data, output_path).await,
        LutFormat::ThreeDL => ThreeDLParser::write(lut_data, output_path).await,
        LutFormat::Lut => CubeParser::write(lut_data, output_path).await,
        LutFormat::Csp => CspParser::write(lut_data, output_path).await,
        LutFormat::M3d => M3dParser::write(lut_data, output_path).await,
        LutFormat::Look => LookParser::write(lut_data, output_path).await,
        LutFormat::Mga => MgaParser::write(lut_data, output_path).await,
        LutFormat::Unknown => Err(AppError::Validation("该 LUT 格式暂不支持导出".to_string())),
    }
}
```

注：
- 如果 `.lut` 已有专用 parser / writer，则使用专用实现；没有的话，在实现时将此步骤中的 `CubeParser` 占位替换为项目内真实可用实现
- 该替换必须在本任务内完成，不能留占位

- [x] **Step 5: 写最小实现，补全 `convert_file`**

把 `convert_file` 改成：

```rust
async fn convert_file(
    &self,
    file_path: &Path,
    target_format: LutFormat,
    options: &ConversionOptions,
) -> AppResult<LutData> {
    let lut_data = self.load_lut_file(file_path).await?;

    if !self.is_conversion_supported(lut_data.format, target_format) {
        return Err(AppError::Validation("源格式与目标格式不兼容".to_string()));
    }

    let converted = self.convert(&lut_data, target_format, options.clone()).await?;
    let output_path = self.build_output_path(file_path, target_format)?;
    self.write_lut_file(&converted, &output_path).await?;

    Ok(converted)
}
```

- [x] **Step 6: 运行测试，确认单文件转换通过**

Run:

```bash
cd /Users/dafuchen/Develop/video/auto-apply-lut/src-tauri
cargo test test_convert_file_writes_converted_file_next_to_source
```

Expected:
- PASS

- [x] **Step 7: 提交**

```bash
cd /Users/dafuchen/Develop/video/auto-apply-lut
git add src-tauri/src/core/lut/converter.rs
git commit -m "feat: implement single lut file conversion"
```

---

### Task 3: 补命名冲突与不兼容格式测试

**Files:**
- Modify: `src-tauri/src/core/lut/converter.rs`
- Test: `src-tauri/src/core/lut/converter.rs`

- [x] **Step 1: 写失败测试，锁定重名时自动追加后缀**

在 `src-tauri/src/core/lut/converter.rs` 测试模块中新增：

```rust
#[tokio::test]
async fn test_convert_file_appends_incrementing_suffix_when_target_exists() {
    use tempfile::tempdir;
    use tokio::fs;

    let converter = LutConverter::new();
    let dir = tempdir().expect("temp dir");
    let source_path = dir.path().join("sample.cube");
    let existing_output = dir.path().join("sample.converted.csp");

    fs::write(
        &source_path,
        r#"TITLE "Sample"
LUT_3D_SIZE 2
0.0 0.0 0.0
1.0 0.0 0.0
0.0 1.0 0.0
1.0 1.0 0.0
0.0 0.0 1.0
1.0 0.0 1.0
0.0 1.0 1.0
1.0 1.0 1.0
"#,
    )
    .await
    .expect("write source");

    fs::write(&existing_output, "occupied").await.expect("write occupied");

    converter
        .convert_file(&source_path, LutFormat::Csp, &ConversionOptions::default())
        .await
        .expect("convert file");

    assert!(dir.path().join("sample.converted-1.csp").exists());
}
```

- [x] **Step 2: 写失败测试，锁定不兼容格式时明确失败**

在同一测试模块中新增：

```rust
#[tokio::test]
async fn test_convert_file_rejects_cross_dimension_conversion() {
    use tempfile::tempdir;
    use tokio::fs;

    let converter = LutConverter::new();
    let dir = tempdir().expect("temp dir");
    let source_path = dir.path().join("sample.cube");

    fs::write(
        &source_path,
        r#"TITLE "Sample"
LUT_3D_SIZE 2
0.0 0.0 0.0
1.0 0.0 0.0
0.0 1.0 0.0
1.0 1.0 0.0
0.0 0.0 1.0
1.0 0.0 1.0
0.0 1.0 1.0
1.0 1.0 1.0
"#,
    )
    .await
    .expect("write source");

    let err = converter
        .convert_file(&source_path, LutFormat::Mga, &ConversionOptions::default())
        .await
        .expect_err("should reject");

    assert!(err.to_string().contains("不兼容"));
}
```

- [x] **Step 3: 运行测试，确认至少有一条失败**

Run:

```bash
cd /Users/dafuchen/Develop/video/auto-apply-lut/src-tauri
cargo test test_convert_file_appends_incrementing_suffix_when_target_exists
cargo test test_convert_file_rejects_cross_dimension_conversion
```

Expected:
- 至少一条 FAIL
- 如果命名冲突逻辑已生效，则第二条仍应暴露不兼容错误文案问题；反之亦然

- [x] **Step 4: 写最小实现，补齐冲突与错误文案**

根据失败结果，仅做最小修复：

```rust
if !self.is_conversion_supported(lut_data.format, target_format) {
    return Err(AppError::Validation("源格式与目标格式不兼容".to_string()));
}
```

并确认 `build_output_path()` 在目标已存在时产生 `-1`, `-2`, `-3` 递增路径。

- [x] **Step 5: 运行测试，确认通过**

Run:

```bash
cd /Users/dafuchen/Develop/video/auto-apply-lut/src-tauri
cargo test test_convert_file_appends_incrementing_suffix_when_target_exists
cargo test test_convert_file_rejects_cross_dimension_conversion
```

Expected:
- PASS

- [x] **Step 6: 提交**

```bash
cd /Users/dafuchen/Develop/video/auto-apply-lut
git add src-tauri/src/core/lut/converter.rs
git commit -m "feat: add lut conversion output conflict handling"
```

---

### Task 4: 打通批量转换命令

**Files:**
- Modify: `src-tauri/src/commands/lut_manager.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/commands/lut_manager.rs`

- [x] **Step 1: 写失败测试，锁定批量命令的聚合结果**

在 `src-tauri/src/commands/lut_manager.rs` 测试模块中新增：

```rust
#[test]
fn batch_convert_response_counts_success_and_failure() {
    use crate::types::lut_conversion::BatchConvertLutItemResult;

    let results = vec![
        BatchConvertLutItemResult {
            source_path: "/tmp/a.cube".to_string(),
            target_path: Some("/tmp/a.converted.csp".to_string()),
            success: true,
            error_message: None,
        },
        BatchConvertLutItemResult {
            source_path: "/tmp/b.cube".to_string(),
            target_path: None,
            success: false,
            error_message: Some("源格式与目标格式不兼容".to_string()),
        },
    ];

    let success_count = results.iter().filter(|item| item.success).count();
    let failure_count = results.iter().filter(|item| !item.success).count();

    assert_eq!(success_count, 1);
    assert_eq!(failure_count, 1);
}
```

- [x] **Step 2: 运行测试，确认当前命令尚不存在**

Run:

```bash
cd /Users/dafuchen/Develop/video/auto-apply-lut/src-tauri
cargo test batch_convert_response_counts_success_and_failure
```

Expected:
- FAIL 或仅通过聚合测试但无法调用命令
- 说明批量命令尚未落地

- [x] **Step 3: 写最小实现，增加批量转换命令**

在 `src-tauri/src/commands/lut_manager.rs` 中新增：

```rust
#[tauri::command]
pub async fn batch_convert_luts(
    request: crate::types::BatchConvertLutsRequest,
) -> Result<crate::types::BatchConvertLutsResponse, String> {
    use crate::core::lut::converter::{ConversionOptions, LutConverter};
    use std::path::Path;

    let converter = LutConverter::new();
    let paths: Vec<&Path> = request.paths.iter().map(Path::new).collect();
    let results = converter
        .batch_convert(paths, request.target_format, ConversionOptions::default())
        .await
        .map_err(|e| e.to_string())?;

    let mapped: Vec<crate::types::BatchConvertLutItemResult> = results
        .into_iter()
        .map(|item| crate::types::BatchConvertLutItemResult {
            source_path: item.source_path.to_string_lossy().to_string(),
            target_path: item
                .converted_data
                .as_ref()
                .map(|_| String::new()),
            success: item.success,
            error_message: item.error,
        })
        .collect();

    let success_count = mapped.iter().filter(|item| item.success).count();
    let failure_count = mapped.iter().filter(|item| !item.success).count();

    Ok(crate::types::BatchConvertLutsResponse {
        success_count,
        failure_count,
        results: mapped,
    })
}
```

注：
- 如果 Task 2/3 中扩展了 `ConversionResult` 以携带真实 `output_path`，则这里必须返回真实输出路径，不能保留 `String::new()`
- 实施时应同步把 `ConversionResult` 设计补齐到可直接映射 `target_path`

- [x] **Step 4: 在 `src-tauri/src/lib.rs` 注册命令**

在 `generate_handler![]` 的 LUT 区域加入：

```rust
commands::lut_manager::batch_convert_luts,
```

- [x] **Step 5: 运行测试，确认命令层通过**

Run:

```bash
cd /Users/dafuchen/Develop/video/auto-apply-lut/src-tauri
cargo test batch_convert_response_counts_success_and_failure
```

Expected:
- PASS

- [x] **Step 6: 提交**

```bash
cd /Users/dafuchen/Develop/video/auto-apply-lut
git add src-tauri/src/commands/lut_manager.rs src-tauri/src/lib.rs
git commit -m "feat: add lut batch conversion command"
```

---

### Task 5: 为资料库面板增加批量转换交互

**Files:**
- Modify: `src/components/LutLibraryPanel.tsx`
- Modify: `src/components/LutLibraryPanel.css`
- Test: `src/components/LutLibraryPanel.test.tsx`

- [x] **Step 1: 写失败测试，锁定按钮显示与禁用状态**

在 `src/components/LutLibraryPanel.test.tsx` 中新增：

```tsx
it('未选中 LUT 时禁用批量转换按钮', async () => {
  invokeMock.mockImplementation(async (command: string) => {
    if (command === 'list_lut_library') return [];
    return [];
  });

  render(
    <LutLibraryPanel
      activeVideoPath={null}
      selectedLutPaths={[]}
      onSelectedLutPathsChange={vi.fn()}
    />
  );

  expect(screen.getByRole('button', { name: '批量转换' })).toBeDisabled();
});
```

- [x] **Step 2: 写失败测试，锁定已选中时可打开转换弹窗**

继续添加：

```tsx
it('选中 LUT 后可以打开批量转换弹窗', async () => {
  invokeMock.mockImplementation(async (command: string) => {
    if (command === 'list_lut_library') {
      return [
        {
          path: '/tmp/a.cube',
          name: 'a.cube',
          size: 128,
          lut_type: 'ThreeDimensional',
          format: 'CUBE',
          category: '3D LUT',
          is_valid: true,
          updated_at: new Date().toISOString(),
          error_message: null,
        },
      ];
    }
    if (command === 'remember_lut_files') return [];
    if (command === 'generate_lut_preview') return '/tmp/preview.png';
    return [];
  });

  render(
    <LutLibraryPanel
      activeVideoPath={null}
      selectedLutPaths={['/tmp/a.cube']}
      onSelectedLutPathsChange={vi.fn()}
    />
  );

  fireEvent.click(await screen.findByRole('button', { name: '批量转换' }));

  expect(screen.getByText('批量转换 LUT')).toBeInTheDocument();
  expect(screen.getByText('已选 1 个文件')).toBeInTheDocument();
});
```

- [x] **Step 3: 运行测试，确认失败**

Run:

```bash
cd /Users/dafuchen/Develop/video/auto-apply-lut
pnpm test src/components/LutLibraryPanel.test.tsx
```

Expected:
- FAIL
- 原因是 UI 中还没有 `批量转换` 按钮或转换弹窗

- [x] **Step 4: 写最小实现，增加前端状态与按钮**

在 `src/components/LutLibraryPanel.tsx` 中新增：

```tsx
const [isBatchConvertOpen, setIsBatchConvertOpen] = useState(false);
const [batchConvertTargetFormat, setBatchConvertTargetFormat] = useState('');
const [isBatchConverting, setIsBatchConverting] = useState(false);
const [batchConvertResult, setBatchConvertResult] = useState<null | {
  success_count: number;
  failure_count: number;
  results: Array<{
    source_path: string;
    target_path?: string | null;
    success: boolean;
    error_message?: string | null;
  }>;
}>(null);
```

在头部操作区加入：

```tsx
<button
  className="lut-library-button"
  type="button"
  disabled={disabled || selectedItems.length === 0}
  onClick={() => setIsBatchConvertOpen(true)}
>
  批量转换
</button>
```

- [x] **Step 5: 写最小实现，增加转换弹窗骨架**

在 `LutLibraryPanel.tsx` 返回 JSX 中加入：

```tsx
{isBatchConvertOpen && (
  <div className="lut-batch-convert-overlay" onClick={() => setIsBatchConvertOpen(false)}>
    <div className="lut-batch-convert-dialog" onClick={(event) => event.stopPropagation()}>
      <div className="lut-batch-convert-header">
        <h3>批量转换 LUT</h3>
        <button type="button" className="btn-close" onClick={() => setIsBatchConvertOpen(false)}>
          ×
        </button>
      </div>
      <div className="lut-batch-convert-body">
        <p>{`已选 ${selectedItems.length} 个文件`}</p>
      </div>
    </div>
  </div>
)}
```

在 `src/components/LutLibraryPanel.css` 中增加最小样式：

```css
.lut-batch-convert-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.4);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1200;
}

.lut-batch-convert-dialog {
  width: min(520px, calc(100vw - 32px));
  border-radius: 16px;
  background: var(--surface-card);
  border: 1px solid var(--border-soft);
  box-shadow: var(--shadow-soft);
  padding: 20px;
}

.lut-batch-convert-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.lut-batch-convert-body {
  margin-top: 16px;
}
```

- [x] **Step 6: 运行测试，确认通过**

Run:

```bash
cd /Users/dafuchen/Develop/video/auto-apply-lut
pnpm test src/components/LutLibraryPanel.test.tsx
```

Expected:
- PASS

- [x] **Step 7: 提交**

```bash
cd /Users/dafuchen/Develop/video/auto-apply-lut
git add src/components/LutLibraryPanel.tsx src/components/LutLibraryPanel.css src/components/LutLibraryPanel.test.tsx
git commit -m "feat: add lut batch conversion panel entry"
```

---

### Task 6: 接入目标格式计算与转换结果展示

**Files:**
- Modify: `src/components/LutLibraryPanel.tsx`
- Modify: `src/components/LutLibraryPanel.css`
- Test: `src/components/LutLibraryPanel.test.tsx`

- [x] **Step 1: 写失败测试，锁定混合维度时禁止转换**

在 `src/components/LutLibraryPanel.test.tsx` 中新增：

```tsx
it('混合维度 LUT 时禁止批量转换', async () => {
  invokeMock.mockImplementation(async (command: string) => {
    if (command === 'list_lut_library') {
      return [
        {
          path: '/tmp/a.cube',
          name: 'a.cube',
          size: 128,
          lut_type: 'ThreeDimensional',
          format: 'CUBE',
          category: '3D LUT',
          is_valid: true,
          updated_at: new Date().toISOString(),
          error_message: null,
        },
        {
          path: '/tmp/b.mga',
          name: 'b.mga',
          size: 64,
          lut_type: 'OneDimensional',
          format: 'MGA',
          category: '1D LUT',
          is_valid: true,
          updated_at: new Date().toISOString(),
          error_message: null,
        },
      ];
    }
    if (command === 'remember_lut_files') return [];
    if (command === 'generate_lut_preview') return '/tmp/preview.png';
    return [];
  });

  render(
    <LutLibraryPanel
      activeVideoPath={null}
      selectedLutPaths={['/tmp/a.cube', '/tmp/b.mga']}
      onSelectedLutPathsChange={vi.fn()}
    />
  );

  fireEvent.click(await screen.findByRole('button', { name: '批量转换' }));

  expect(
    screen.getByText('当前选中项包含不同维度的 LUT，无法批量转换到同一目标格式')
  ).toBeInTheDocument();
});
```

- [x] **Step 2: 写失败测试，锁定成功结果摘要展示**

继续添加：

```tsx
it('批量转换后展示成功与失败摘要', async () => {
  invokeMock.mockImplementation(async (command: string) => {
    if (command === 'list_lut_library') {
      return [
        {
          path: '/tmp/a.cube',
          name: 'a.cube',
          size: 128,
          lut_type: 'ThreeDimensional',
          format: 'CUBE',
          category: '3D LUT',
          is_valid: true,
          updated_at: new Date().toISOString(),
          error_message: null,
        },
      ];
    }
    if (command === 'remember_lut_files') return [];
    if (command === 'generate_lut_preview') return '/tmp/preview.png';
    if (command === 'batch_convert_luts') {
      return {
        success_count: 1,
        failure_count: 0,
        results: [
          {
            source_path: '/tmp/a.cube',
            target_path: '/tmp/a.converted.csp',
            success: true,
            error_message: null,
          },
        ],
      };
    }
    return [];
  });

  render(
    <LutLibraryPanel
      activeVideoPath={null}
      selectedLutPaths={['/tmp/a.cube']}
      onSelectedLutPathsChange={vi.fn()}
    />
  );

  fireEvent.click(await screen.findByRole('button', { name: '批量转换' }));
  fireEvent.click(screen.getByRole('button', { name: '开始转换' }));

  expect(await screen.findByText('成功 1 个，失败 0 个')).toBeInTheDocument();
});
```

- [x] **Step 3: 运行测试，确认失败**

Run:

```bash
cd /Users/dafuchen/Develop/video/auto-apply-lut
pnpm test src/components/LutLibraryPanel.test.tsx
```

Expected:
- FAIL
- 因为可选目标格式、混合维度拦截和结果摘要还未实现

- [x] **Step 4: 写最小实现，计算共同目标格式**

在 `LutLibraryPanel.tsx` 中增加纯前端推导逻辑：

```tsx
const supportedTargetsByFormat: Record<string, string[]> = {
  CUBE: ['3DL', 'CSP', 'M3D', 'LOOK'],
  '3DL': ['CUBE', 'CSP', 'M3D'],
  CSP: ['CUBE', '3DL', 'M3D'],
  M3D: ['CUBE', '3DL', 'CSP'],
  LOOK: ['CUBE'],
  LUT: ['MGA'],
  MGA: ['LUT'],
};

const batchConvertState = useMemo(() => {
  if (selectedItems.length === 0) {
    return { availableFormats: [] as string[], error: '请先选择 LUT 文件' };
  }

  const dimensions = new Set(
    selectedItems
      .filter(item => !item.is_placeholder && item.is_valid)
      .map(item => item.lut_type)
  );

  if (dimensions.size > 1) {
    return {
      availableFormats: [] as string[],
      error: '当前选中项包含不同维度的 LUT，无法批量转换到同一目标格式',
    };
  }

  const intersections = selectedItems
    .map(item => supportedTargetsByFormat[item.format] ?? [])
    .reduce<string[]>((acc, current, index) => {
      if (index === 0) return current;
      return acc.filter(value => current.includes(value));
    }, []);

  return { availableFormats: intersections, error: null as string | null };
}, [selectedItems]);
```

- [x] **Step 5: 写最小实现，接入命令调用与结果展示**

在 `LutLibraryPanel.tsx` 中加入触发逻辑：

```tsx
const handleBatchConvert = useCallback(async () => {
  if (!batchConvertTargetFormat || batchConvertState.error || selectedLutPaths.length === 0) return;

  try {
    setIsBatchConverting(true);
    const response = await invoke<{
      success_count: number;
      failure_count: number;
      results: Array<{
        source_path: string;
        target_path?: string | null;
        success: boolean;
        error_message?: string | null;
      }>;
    }>('batch_convert_luts', {
      request: {
        paths: selectedLutPaths,
        target_format: batchConvertTargetFormat.toLowerCase(),
      },
    });

    setBatchConvertResult(response);
    await loadLibrary();
  } catch (err) {
    setError(err instanceof Error ? err.message : '批量转换失败');
  } finally {
    setIsBatchConverting(false);
  }
}, [batchConvertState.error, batchConvertTargetFormat, loadLibrary, selectedLutPaths]);
```

并在弹窗主体中加入：

```tsx
{batchConvertState.error ? (
  <div className="lut-library-error">{batchConvertState.error}</div>
) : (
  <label className="lut-batch-convert-field">
    <span>目标格式</span>
    <select
      value={batchConvertTargetFormat}
      onChange={(event) => setBatchConvertTargetFormat(event.target.value)}
    >
      <option value="">请选择</option>
      {batchConvertState.availableFormats.map(format => (
        <option key={format} value={format}>
          {format}
        </option>
      ))}
    </select>
  </label>
)}

<button
  type="button"
  className="lut-library-button"
  disabled={Boolean(batchConvertState.error) || !batchConvertTargetFormat || isBatchConverting}
  onClick={() => void handleBatchConvert()}
>
  {isBatchConverting ? '转换中...' : '开始转换'}
</button>

{batchConvertResult && (
  <div className="lut-batch-convert-result">
    <div>{`成功 ${batchConvertResult.success_count} 个，失败 ${batchConvertResult.failure_count} 个`}</div>
  </div>
)}
```

- [x] **Step 6: 补最小样式**

在 `src/components/LutLibraryPanel.css` 中增加：

```css
.lut-batch-convert-field {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.lut-batch-convert-field select {
  min-height: 40px;
  border-radius: 10px;
  border: 1px solid var(--border-soft);
  background: var(--surface-elevated);
  color: var(--color-fg);
  padding: 0 12px;
}

.lut-batch-convert-result {
  margin-top: 16px;
  padding: 12px;
  border-radius: 12px;
  border: 1px solid var(--border-soft);
  background: var(--surface-elevated);
}
```

- [x] **Step 7: 运行测试，确认通过**

Run:

```bash
cd /Users/dafuchen/Develop/video/auto-apply-lut
pnpm test src/components/LutLibraryPanel.test.tsx
```

Expected:
- PASS

- [x] **Step 8: 提交**

```bash
cd /Users/dafuchen/Develop/video/auto-apply-lut
git add src/components/LutLibraryPanel.tsx src/components/LutLibraryPanel.css src/components/LutLibraryPanel.test.tsx
git commit -m "feat: add lut batch conversion workflow"
```

---

### Task 7: 整体验证与回归

**Files:**
- Modify: `src/components/LutLibraryPanel.test.tsx`（如需小修）
- Modify: `src-tauri/src/commands/lut_manager.rs`（如需小修）
- Modify: `src-tauri/src/core/lut/converter.rs`（如需小修）

- [x] **Step 1: 运行前端测试**

Run:

```bash
cd /Users/dafuchen/Develop/video/auto-apply-lut
pnpm test
```

Expected:
- PASS

- [x] **Step 2: 运行类型检查**

Run:

```bash
cd /Users/dafuchen/Develop/video/auto-apply-lut
pnpm exec tsc --noEmit
```

Expected:
- PASS

- [x] **Step 3: 运行 Rust 目标测试**

Run:

```bash
cd /Users/dafuchen/Develop/video/auto-apply-lut/src-tauri
cargo test test_convert_file_writes_converted_file_next_to_source
cargo test test_convert_file_appends_incrementing_suffix_when_target_exists
cargo test test_convert_file_rejects_cross_dimension_conversion
cargo test batch_convert_response_contract_is_stable
cargo test batch_convert_response_counts_success_and_failure
```

Expected:
- PASS

- [x] **Step 4: 手动验证**

Run:

```bash
cd /Users/dafuchen/Develop/video/auto-apply-lut
pnpm tauri dev
```

Manual checklist:

- 打开 `LUT 资料库`
- 选择多个同维度 LUT
- 点击 `批量转换`
- 选择合法目标格式
- 确认新文件生成在原文件旁边
- 人工制造同名目标文件，确认生成 `-1` 后缀
- 混合选择 1D / 3D 时，确认按钮或弹窗明确禁止转换
- 确认转换完成后资料库刷新

- [x] **Step 5: 最终提交**

```bash
cd /Users/dafuchen/Develop/video/auto-apply-lut
git add src src-tauri
git commit -m "feat: add lut batch conversion"
```

---

## Self-Review

- **Execution status:** 该功能已经在后端类型、转换器、命令层和前端资料库面板中落地；本文档中的 checklist 已按当前仓库状态完成回填。

### Spec coverage

- 资料库入口：Task 5, Task 6
- 原文件旁输出：Task 2, Task 3
- 自动追加后缀：Task 3
- 单文件失败不影响整体：Task 4, Task 7
- 兼容格式限制：Task 3, Task 6
- 结果摘要与失败展示：Task 6
- 自动刷新资料库：Task 5, Task 6

### Placeholder scan

- 计划中没有 `TBD` / `TODO`
- 唯一需要实施时现场确认的是 `.lut` 对应 parser / writer 的真实实现类名；该点已在 Task 2 明确要求必须在该任务内替换成项目内真实实现，不能留占位

### Type consistency

- 前后端统一使用：
  - `BatchConvertLutsRequest`
  - `BatchConvertLutItemResult`
  - `BatchConvertLutsResponse`
- 前端结果字段统一使用：
  - `success_count`
  - `failure_count`
  - `results`
  - `source_path`
  - `target_path`
  - `error_message`
