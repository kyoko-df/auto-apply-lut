# 设置弹窗系统诊断接入 Implementation Plan

> 状态：已完成实现、测试和类型检查

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 `SettingsModal` 中接通系统信息、缓存管理和日志查看，让前端真实调用已注册的 Tauri 命令。

**Architecture:** 继续沿用 `SettingsModal.tsx` 里的 `safeInvoke()` 和分区式设置面板，不新增页面。前端通过新增一组系统/缓存/日志状态，把 `get_system_info`、`get_cache_size`、`clear_cache`、`get_log_files`、`read_log_file` 串起来；测试用 Vitest + Testing Library 模拟 `@tauri-apps/api/core` 的 `invoke()` 行为。

**Tech Stack:** React 19, TypeScript, Vitest, Testing Library, Tauri v2

## 完成情况

- 已在 `SettingsModal` 中新增 `系统与缓存`、`日志` 两个分区
- 已接通 `get_system_info`、`get_cache_size`、`clear_cache`、`get_log_files`、`read_log_file`
- 已修复 `safeInvoke()` 将所有异常误判为 `tauri_unavailable` 的回归问题
- 已补充 `SettingsModal.test.tsx` 测试覆盖核心交互、降级提示与错误态
- 已通过 `pnpm test -- SettingsModal.test.tsx`
- 已通过 `pnpm exec tsc --noEmit`

---

## File Map

- Modify: `src/components/SettingsModal.tsx`
  - 新增系统信息、缓存大小、日志列表、日志内容的状态
  - 在弹窗打开时拉取系统与缓存数据
  - 增加清理缓存、加载日志列表、读取日志文件的回调
  - 在现有 `高级设置` 区域后追加 `系统与缓存`、`日志` 两个 section
- Modify: `src/components/SettingsModal.css`
  - 新增系统摘要、缓存操作区、日志列表、日志内容预览区样式
  - 约束日志预览高度，避免撑爆弹窗
- Create: `src/components/SettingsModal.test.tsx`
  - 覆盖弹窗打开加载系统/缓存、清理缓存刷新、日志列表读取、日志内容展示、Tauri 降级提示

### Task 1: 建立系统与缓存加载回归测试

**Files:**
- Create: `src/components/SettingsModal.test.tsx`
- Modify: `src/components/SettingsModal.tsx`

- [x] **Step 1: 写失败测试，验证弹窗打开时会请求系统信息和缓存大小**

```tsx
import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import SettingsModal from './SettingsModal';

const invokeMock = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

describe('SettingsModal', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    (window as any).__TAURI_INTERNALS__ = {};
  });

  it('打开弹窗后加载系统信息与缓存大小', async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === 'get_available_codecs') {
        return { video_codecs: [], audio_codecs: [] };
      }
      if (command === 'get_gpu_info') return [];
      if (command === 'check_hardware_acceleration') {
        return { available: false, supported_codecs: [], recommended_settings: [] };
      }
      if (command === 'get_system_info') {
        return {
          cpu_usage: 0,
          memory_usage: 42.5,
          total_memory: 17179869184,
          available_memory: 8589934592,
          disk_usage: [],
          cpu_count: 10,
          system_name: 'macOS',
          system_version: '15.0'
        };
      }
      if (command === 'get_cache_size') return 1048576;
      return null;
    });

    render(
      <SettingsModal
        isOpen
        settings={{
          output_format: 'mp4',
          video_codec: 'libx264',
          audio_codec: 'aac',
          quality_preset: 'balanced',
          resolution: 'original',
          fps: null,
          bitrate: 'auto',
          lut_intensity: 100,
          lut_error_strategy: 'StopOnError',
          color_space: 'rec709',
          hardware_acceleration: false,
          two_pass_encoding: false,
          preserve_metadata: true,
          output_directory: ''
        }}
        onClose={vi.fn()}
        onSettingsChange={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('get_system_info', undefined);
      expect(invokeMock).toHaveBeenCalledWith('get_cache_size', undefined);
    });

    expect(await screen.findByText('系统与缓存')).toBeInTheDocument();
    expect(screen.getByText('macOS 15.0')).toBeInTheDocument();
    expect(screen.getByText('1.0 MB')).toBeInTheDocument();
  });
});
```

- [x] **Step 2: 跑单测，确认它先失败**

Run:

```bash
pnpm test -- SettingsModal.test.tsx
```

Expected:

```text
FAIL  src/components/SettingsModal.test.tsx
TestingLibraryElementError: Unable to find an element with the text: 系统与缓存
```

- [x] **Step 3: 在 `SettingsModal.tsx` 中加入系统/缓存类型、状态和加载函数**

```tsx
interface SystemInfo {
  cpu_usage: number;
  memory_usage: number;
  total_memory: number;
  available_memory: number;
  disk_usage: Array<{
    name: string;
    mount_point: string;
    total_space: number;
    available_space: number;
    usage_percentage: number;
  }>;
  cpu_count: number;
  system_name: string;
  system_version: string;
}

const [systemInfo, setSystemInfo] = useState<SystemInfo | null>(null);
const [systemInfoError, setSystemInfoError] = useState<string | null>(null);
const [systemInfoLoading, setSystemInfoLoading] = useState(false);
const [cacheSize, setCacheSize] = useState<number | null>(null);
const [cacheError, setCacheError] = useState<string | null>(null);
const [cacheLoading, setCacheLoading] = useState(false);

const formatBytes = useCallback((bytes?: number | null): string => {
  if (bytes == null || bytes < 0) return '未知';
  if (bytes === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value.toFixed(unitIndex === 0 ? 0 : 1)} ${units[unitIndex]}`;
}, []);

const loadSystemDiagnostics = useCallback(async () => {
  try {
    setSystemInfoLoading(true);
    setCacheLoading(true);
    setSystemInfoError(null);
    setCacheError(null);

    const [info, size] = await Promise.all([
      safeInvoke<SystemInfo>('get_system_info'),
      safeInvoke<number>('get_cache_size')
    ]);

    setSystemInfo(info);
    setCacheSize(size);
  } catch (error) {
    const message = error instanceof Error ? error.message : 'unknown_error';
    if (message === 'tauri_unavailable') {
      setSystemInfoError('当前环境不支持系统诊断（浏览器预览模式）');
      setCacheError('当前环境不支持缓存诊断（浏览器预览模式）');
    } else {
      setSystemInfoError('系统信息加载失败');
      setCacheError('缓存信息加载失败');
    }
  } finally {
    setSystemInfoLoading(false);
    setCacheLoading(false);
  }
}, []);
```

- [x] **Step 4: 在弹窗打开的 `useEffect` 中接入加载逻辑，并渲染系统与缓存区**

```tsx
useEffect(() => {
  if (isOpen) {
    loadAvailableCodecs();
    loadGpuDiagnostics();
    loadSystemDiagnostics();
    setCodecTestResult(null);
  }
}, [isOpen, loadAvailableCodecs, loadGpuDiagnostics, loadSystemDiagnostics]);
```

```tsx
<div className="settings-section">
  <h3>系统与缓存</h3>

  <div className="setting-group diagnostics-card">
    <div className="diagnostics-header">
      <span className="setting-label">运行环境</span>
      <button
        className="btn-secondary btn-small"
        onClick={loadSystemDiagnostics}
        disabled={disabled || systemInfoLoading || cacheLoading}
        type="button"
      >
        {systemInfoLoading || cacheLoading ? '刷新中...' : '刷新'}
      </button>
    </div>

    {systemInfoError && <div className="gpu-error">{systemInfoError}</div>}
    {!systemInfoError && systemInfo && (
      <div className="diagnostics-list">
        <div className="diagnostics-row">系统: {systemInfo.system_name} {systemInfo.system_version}</div>
        <div className="diagnostics-row">CPU 核心: {systemInfo.cpu_count}</div>
        <div className="diagnostics-row">内存占用: {systemInfo.memory_usage.toFixed(1)}%</div>
      </div>
    )}

    <div className="cache-row">
      <div className="diagnostics-row">缓存大小: {cacheLoading ? '读取中...' : formatBytes(cacheSize)}</div>
    </div>
  </div>
</div>
```

- [x] **Step 5: 重新跑单测，确认通过**

Run:

```bash
pnpm test -- SettingsModal.test.tsx
```

Expected:

```text
PASS  src/components/SettingsModal.test.tsx
  ✓ 打开弹窗后加载系统信息与缓存大小
```

- [ ] **Step 6: 提交**

```bash
git add src/components/SettingsModal.tsx src/components/SettingsModal.test.tsx
git commit -m "test: cover settings diagnostics bootstrap"
```

### Task 2: 接通清理缓存并回刷缓存大小

**Files:**
- Modify: `src/components/SettingsModal.test.tsx`
- Modify: `src/components/SettingsModal.tsx`

- [x] **Step 1: 写失败测试，验证点击清理缓存后会重新请求缓存大小**

```tsx
it('清理缓存后刷新缓存大小', async () => {
  let cacheReads = 0;

  invokeMock.mockImplementation(async (command: string) => {
    if (command === 'get_available_codecs') {
      return { video_codecs: [], audio_codecs: [] };
    }
    if (command === 'get_gpu_info') return [];
    if (command === 'check_hardware_acceleration') {
      return { available: false, supported_codecs: [], recommended_settings: [] };
    }
    if (command === 'get_system_info') {
      return {
        cpu_usage: 0,
        memory_usage: 12,
        total_memory: 17179869184,
        available_memory: 8589934592,
        disk_usage: [],
        cpu_count: 8,
        system_name: 'macOS',
        system_version: '15.0'
      };
    }
    if (command === 'get_cache_size') {
      cacheReads += 1;
      return cacheReads === 1 ? 3145728 : 0;
    }
    if (command === 'clear_cache') {
      return 'Cache cleared successfully';
    }
    return null;
  });

  render(<SettingsModal {...baseProps} />);

  expect(await screen.findByText('3.0 MB')).toBeInTheDocument();

  fireEvent.click(screen.getByRole('button', { name: '清理缓存' }));

  await waitFor(() => {
    expect(invokeMock).toHaveBeenCalledWith('clear_cache', undefined);
  });

  expect(await screen.findByText('0 B')).toBeInTheDocument();
});
```

- [x] **Step 2: 跑单测，确认它因为缺少按钮或刷新逻辑而失败**

Run:

```bash
pnpm test -- SettingsModal.test.tsx -t "清理缓存后刷新缓存大小"
```

Expected:

```text
FAIL
Unable to find role="button" and name "清理缓存"
```

- [x] **Step 3: 在 `SettingsModal.tsx` 中实现缓存清理逻辑**

```tsx
const [cacheActionLoading, setCacheActionLoading] = useState(false);
const [cacheActionMessage, setCacheActionMessage] = useState<string | null>(null);

const refreshCacheSize = useCallback(async () => {
  try {
    setCacheLoading(true);
    setCacheError(null);
    const size = await safeInvoke<number>('get_cache_size');
    setCacheSize(size);
  } catch (error) {
    const message = error instanceof Error ? error.message : 'unknown_error';
    setCacheError(
      message === 'tauri_unavailable'
        ? '当前环境不支持缓存诊断（浏览器预览模式）'
        : '缓存信息加载失败'
    );
  } finally {
    setCacheLoading(false);
  }
}, []);

const handleClearCache = useCallback(async () => {
  try {
    setCacheActionLoading(true);
    setCacheActionMessage(null);
    await safeInvoke<string>('clear_cache');
    setCacheActionMessage('缓存已清理');
    await refreshCacheSize();
  } catch (error) {
    const message = error instanceof Error ? error.message : 'unknown_error';
    setCacheActionMessage(
      message === 'tauri_unavailable'
        ? '当前环境不支持缓存清理（浏览器预览模式）'
        : '清理缓存失败'
    );
  } finally {
    setCacheActionLoading(false);
  }
}, [refreshCacheSize]);
```

- [x] **Step 4: 渲染清理缓存按钮和反馈文案**

```tsx
<div className="cache-actions">
  <button
    className="btn-secondary btn-small"
    onClick={handleClearCache}
    disabled={disabled || cacheActionLoading || cacheLoading}
    type="button"
  >
    {cacheActionLoading ? '清理中...' : '清理缓存'}
  </button>
  {cacheActionMessage && (
    <span className="cache-action-message">{cacheActionMessage}</span>
  )}
</div>
```

- [x] **Step 5: 跑单测，确认缓存清理链路通过**

Run:

```bash
pnpm test -- SettingsModal.test.tsx -t "清理缓存后刷新缓存大小"
```

Expected:

```text
PASS
  ✓ 清理缓存后刷新缓存大小
```

- [ ] **Step 6: 提交**

```bash
git add src/components/SettingsModal.tsx src/components/SettingsModal.test.tsx
git commit -m "feat: add cache controls to settings modal"
```

### Task 3: 接通日志列表与日志内容预览

**Files:**
- Modify: `src/components/SettingsModal.test.tsx`
- Modify: `src/components/SettingsModal.tsx`
- Modify: `src/components/SettingsModal.css`

- [x] **Step 1: 写失败测试，验证日志列表加载和内容展示**

```tsx
it('加载日志列表并展示选中文件内容', async () => {
  invokeMock.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
    if (command === 'get_available_codecs') {
      return { video_codecs: [], audio_codecs: [] };
    }
    if (command === 'get_gpu_info') return [];
    if (command === 'check_hardware_acceleration') {
      return { available: false, supported_codecs: [], recommended_settings: [] };
    }
    if (command === 'get_system_info') {
      return {
        cpu_usage: 0,
        memory_usage: 12,
        total_memory: 17179869184,
        available_memory: 8589934592,
        disk_usage: [],
        cpu_count: 8,
        system_name: 'macOS',
        system_version: '15.0'
      };
    }
    if (command === 'get_cache_size') return 0;
    if (command === 'get_log_files') {
      return ['app.log', 'worker.log'];
    }
    if (command === 'read_log_file') {
      expect(args).toEqual({ fileName: 'app.log' });
      return '2026-04-24 10:00:00 INFO boot ok';
    }
    return null;
  });

  render(<SettingsModal {...baseProps} />);

  fireEvent.click(await screen.findByRole('button', { name: '加载日志' }));

  expect(await screen.findByRole('button', { name: 'app.log' })).toBeInTheDocument();

  fireEvent.click(screen.getByRole('button', { name: 'app.log' }));

  expect(await screen.findByText('2026-04-24 10:00:00 INFO boot ok')).toBeInTheDocument();
});
```

- [x] **Step 2: 跑单测，确认日志区目前还不存在**

Run:

```bash
pnpm test -- SettingsModal.test.tsx -t "加载日志列表并展示选中文件内容"
```

Expected:

```text
FAIL
Unable to find role="button" and name "加载日志"
```

- [x] **Step 3: 在 `SettingsModal.tsx` 中加入日志状态与加载回调**

```tsx
const [logFiles, setLogFiles] = useState<string[]>([]);
const [logFilesLoading, setLogFilesLoading] = useState(false);
const [logFilesError, setLogFilesError] = useState<string | null>(null);
const [selectedLogFile, setSelectedLogFile] = useState<string | null>(null);
const [logContent, setLogContent] = useState<string>('');
const [logContentLoading, setLogContentLoading] = useState(false);
const [logContentError, setLogContentError] = useState<string | null>(null);
const [hasLoadedLogs, setHasLoadedLogs] = useState(false);

const loadLogFiles = useCallback(async () => {
  try {
    setLogFilesLoading(true);
    setLogFilesError(null);
    const files = await safeInvoke<string[]>('get_log_files');
    setLogFiles(files);
    setHasLoadedLogs(true);
  } catch (error) {
    const message = error instanceof Error ? error.message : 'unknown_error';
    setLogFilesError(
      message === 'tauri_unavailable'
        ? '当前环境不支持日志查看（浏览器预览模式）'
        : '日志列表加载失败'
    );
  } finally {
    setLogFilesLoading(false);
  }
}, []);

const loadLogContent = useCallback(async (fileName: string) => {
  try {
    setSelectedLogFile(fileName);
    setLogContentLoading(true);
    setLogContentError(null);
    const content = await safeInvoke<string>('read_log_file', { fileName });
    setLogContent(content);
  } catch (error) {
    const message = error instanceof Error ? error.message : 'unknown_error';
    setLogContentError(
      message === 'tauri_unavailable'
        ? '当前环境不支持日志查看（浏览器预览模式）'
        : '日志内容读取失败'
    );
  } finally {
    setLogContentLoading(false);
  }
}, []);
```

- [x] **Step 4: 渲染日志区**

```tsx
<div className="settings-section">
  <h3>日志</h3>

  <div className="setting-group diagnostics-card">
    <div className="diagnostics-header">
      <span className="setting-label">应用日志</span>
      <button
        className="btn-secondary btn-small"
        onClick={loadLogFiles}
        disabled={disabled || logFilesLoading}
        type="button"
      >
        {logFilesLoading ? '加载中...' : '加载日志'}
      </button>
    </div>

    {logFilesError && <div className="gpu-error">{logFilesError}</div>}
    {!logFilesError && hasLoadedLogs && logFiles.length === 0 && (
      <div className="diagnostics-row">暂无日志文件</div>
    )}

    {logFiles.length > 0 && (
      <div className="log-browser">
        <div className="log-file-list">
          {logFiles.map((file) => (
            <button
              key={file}
              type="button"
              className={`log-file-item ${selectedLogFile === file ? 'active' : ''}`}
              onClick={() => void loadLogContent(file)}
            >
              {file}
            </button>
          ))}
        </div>
        <pre className="log-content">
          {logContentLoading ? '正在读取日志...' : logContent || '请选择日志文件'}
        </pre>
      </div>
    )}

    {logContentError && <div className="gpu-error">{logContentError}</div>}
  </div>
</div>
```

- [x] **Step 5: 在 `SettingsModal.css` 中补日志区样式**

```css
.settings-modal .diagnostics-card {
  margin-top: 8px;
  padding: 12px;
  border: 1px solid rgba(0, 0, 0, 0.1);
  border-radius: 10px;
  background: rgba(0, 122, 255, 0.04);
}

.settings-modal .diagnostics-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.settings-modal .diagnostics-row {
  font-size: 12px;
  color: #4b5563;
  word-break: break-word;
}

.settings-modal .log-browser {
  display: grid;
  gap: 10px;
}

.settings-modal .log-file-list {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.settings-modal .log-file-item {
  padding: 6px 10px;
  border: 1px solid rgba(0, 0, 0, 0.12);
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.75);
  cursor: pointer;
}

.settings-modal .log-file-item.active {
  border-color: #007aff;
  color: #007aff;
}

.settings-modal .log-content {
  max-height: 220px;
  overflow: auto;
  padding: 12px;
  border-radius: 8px;
  background: rgba(15, 23, 42, 0.92);
  color: #e5e7eb;
  font: 12px/1.5 SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  white-space: pre-wrap;
}
```

- [x] **Step 6: 跑日志相关测试，确认通过**

Run:

```bash
pnpm test -- SettingsModal.test.tsx -t "加载日志列表并展示选中文件内容"
```

Expected:

```text
PASS
  ✓ 加载日志列表并展示选中文件内容
```

- [ ] **Step 7: 提交**

```bash
git add src/components/SettingsModal.tsx src/components/SettingsModal.css src/components/SettingsModal.test.tsx
git commit -m "feat: add settings log viewer"
```

### Task 4: 补降级与错误态测试，收尾验证

**Files:**
- Modify: `src/components/SettingsModal.test.tsx`
- Modify: `src/components/SettingsModal.tsx`
- Modify: `src/components/SettingsModal.css`

- [x] **Step 1: 写失败测试，验证 Tauri 不可用时会显示明确提示**

```tsx
it('浏览器预览模式下显示系统诊断降级提示', async () => {
  delete (window as any).__TAURI_INTERNALS__;

  render(<SettingsModal {...baseProps} />);

  expect(
    await screen.findByText('当前环境不支持系统诊断（浏览器预览模式）')
  ).toBeInTheDocument();
});
```

- [x] **Step 2: 写失败测试，验证日志读取失败时仍保留日志列表**

```tsx
it('日志读取失败时保留文件列表并显示错误', async () => {
  invokeMock.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
    if (command === 'get_available_codecs') {
      return { video_codecs: [], audio_codecs: [] };
    }
    if (command === 'get_gpu_info') return [];
    if (command === 'check_hardware_acceleration') {
      return { available: false, supported_codecs: [], recommended_settings: [] };
    }
    if (command === 'get_system_info') {
      return {
        cpu_usage: 0,
        memory_usage: 10,
        total_memory: 17179869184,
        available_memory: 8589934592,
        disk_usage: [],
        cpu_count: 8,
        system_name: 'macOS',
        system_version: '15.0'
      };
    }
    if (command === 'get_cache_size') return 0;
    if (command === 'get_log_files') return ['app.log'];
    if (command === 'read_log_file' && args?.fileName === 'app.log') {
      throw new Error('disk error');
    }
    return null;
  });

  render(<SettingsModal {...baseProps} />);

  fireEvent.click(await screen.findByRole('button', { name: '加载日志' }));
  fireEvent.click(await screen.findByRole('button', { name: 'app.log' }));

  expect(await screen.findByText('日志内容读取失败')).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'app.log' })).toBeInTheDocument();
});
```

- [x] **Step 3: 让实现对齐这两个测试**

```tsx
if (message === 'tauri_unavailable') {
  setSystemInfoError('当前环境不支持系统诊断（浏览器预览模式）');
  setCacheError('当前环境不支持缓存诊断（浏览器预览模式）');
}

setLogFiles((current) => current);
setLogContent('');
setLogContentError('日志内容读取失败');
```

```css
.settings-modal .cache-action-message {
  font-size: 12px;
  color: #15803d;
}

@media (prefers-color-scheme: dark) {
  .settings-modal .diagnostics-row {
    color: #d1d5db;
  }

  .settings-modal .log-file-item {
    background: rgba(58, 58, 60, 0.7);
    border-color: rgba(255, 255, 255, 0.12);
    color: #f5f5f7;
  }
}
```

- [x] **Step 4: 跑组件测试、类型检查和诊断**

Run:

```bash
pnpm test -- SettingsModal.test.tsx
pnpm exec tsc --noEmit
```

Expected:

```text
PASS  src/components/SettingsModal.test.tsx
Found 0 errors
```

Then run diagnostics on edited files:

```text
GetDiagnostics for src/components/SettingsModal.tsx
GetDiagnostics for src/components/SettingsModal.css
GetDiagnostics for src/components/SettingsModal.test.tsx
```

Expected:

```text
No new diagnostics
```

- [ ] **Step 5: 跑一次前端回归，确认没有打破现有功能**

Run:

```bash
pnpm test
```

Expected:

```text
All frontend tests pass
```

- [ ] **Step 6: 提交**

```bash
git add src/components/SettingsModal.tsx src/components/SettingsModal.css src/components/SettingsModal.test.tsx
git commit -m "feat: wire settings diagnostics commands"
```

## Self-Review

- **Execution status:** Task 1-4 的实现、针对性测试、类型检查和诊断检查已完成；当前仓库工作区已清洁，本文档说明已同步到最新状态。

- **Spec coverage:** 覆盖了系统信息展示、缓存大小展示、清理缓存、日志列表读取、日志内容预览、Tauri 降级提示，没有遗漏本次 spec 的前端接入范围。
- **Placeholder scan:** 计划中没有 `TODO`、`TBD` 或“稍后处理”类占位词；所有任务都带了具体文件、代码片段、命令和预期结果。
- **Type consistency:** 全程统一使用 `get_system_info`、`get_cache_size`、`clear_cache`、`get_log_files`、`read_log_file`；日志读取参数统一为 `{ fileName }`，与 Tauri 命令参数名保持一致。
