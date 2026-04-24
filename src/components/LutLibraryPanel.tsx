import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import './LutLibraryPanel.css';

interface LutLibraryItem {
  id?: number | null;
  path: string;
  name: string;
  size: number;
  lut_type: string;
  format: string;
  category: string;
  is_valid: boolean;
  error_message?: string | null;
  updated_at: string;
  is_placeholder?: boolean;
}

interface LutLibraryPanelProps {
  activeVideoPath?: string | null;
  selectedLutPaths: string[];
  onSelectedLutPathsChange: (paths: string[]) => void;
  disabled?: boolean;
  onClose?: () => void;
}

interface BatchConvertResult {
  success_count: number;
  failure_count: number;
  results: Array<{
    source_path: string;
    target_path?: string | null;
    success: boolean;
    error_message?: string | null;
  }>;
}

const FORMAT_VARIANTS: Record<string, { value: string; label: string }> = {
  CUBE: { value: 'Cube', label: 'CUBE' },
  '3DL': { value: 'ThreeDL', label: '3DL' },
  CSP: { value: 'Csp', label: 'CSP' },
  M3D: { value: 'M3d', label: 'M3D' },
  LOOK: { value: 'Look', label: 'LOOK' },
  LUT: { value: 'Lut', label: 'LUT' }
};

const SUPPORTED_TARGETS_BY_FORMAT: Record<string, string[]> = {
  CUBE: ['3DL', 'CSP', 'M3D', 'LOOK'],
  '3DL': ['CUBE', 'CSP', 'M3D'],
  CSP: ['CUBE', '3DL', 'M3D'],
  M3D: ['CUBE', '3DL', 'CSP'],
  LOOK: ['CUBE'],
  LUT: []
};

const isTauriEnv = () => typeof window !== 'undefined' && !!(window as any).__TAURI_INTERNALS__;

const formatFileSize = (bytes: number) => {
  const units = ['B', 'KB', 'MB', 'GB'];
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value.toFixed(unitIndex === 0 ? 0 : 1)} ${units[unitIndex]}`;
};

const formatUpdatedAt = (value: string) => {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
};

const LutLibraryPanel: React.FC<LutLibraryPanelProps> = ({
  activeVideoPath,
  selectedLutPaths,
  onSelectedLutPathsChange,
  disabled = false,
  onClose
}) => {
  const [libraryItems, setLibraryItems] = useState<LutLibraryItem[]>([]);
  const [selectedPreviewPath, setSelectedPreviewPath] = useState<string | null>(null);
  const [previewSrc, setPreviewSrc] = useState<string>('');
  const [libraryLoading, setLibraryLoading] = useState(false);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [importing, setImporting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [isBatchConvertOpen, setIsBatchConvertOpen] = useState(false);
  const [batchConvertTargetFormat, setBatchConvertTargetFormat] = useState('');
  const [isBatchConverting, setIsBatchConverting] = useState(false);
  const [batchConvertResult, setBatchConvertResult] = useState<BatchConvertResult | null>(null);

  const selectedSet = useMemo(() => new Set(selectedLutPaths), [selectedLutPaths]);
  const selectedItems = useMemo(() => {
    const byPath = new Map(libraryItems.map(item => [item.path, item] as const));
    return selectedLutPaths.map(path => byPath.get(path) ?? {
      path,
      name: path.split(/[\\/]/).pop() || path,
      size: 0,
      lut_type: 'Loading',
      format: path.split('.').pop()?.toUpperCase() || '',
      category: '正在读取资料',
      is_valid: false,
      updated_at: '',
      error_message: null,
      is_placeholder: true
    });
  }, [libraryItems, selectedLutPaths]);

  const loadLibrary = useCallback(async () => {
    if (!isTauriEnv()) return;
    try {
      setLibraryLoading(true);
      const items = await invoke<LutLibraryItem[]>('list_lut_library');
      setLibraryItems(items ?? []);
      setError(null);
    } catch (err) {
      console.error('Failed to load LUT library:', err);
      setError(err instanceof Error ? err.message : '无法加载 LUT 资料库');
    } finally {
      setLibraryLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadLibrary();
  }, [loadLibrary]);

  useEffect(() => {
    if (!isTauriEnv() || selectedLutPaths.length === 0) return;
    let active = true;

    const syncSelection = async () => {
      try {
        await invoke<LutLibraryItem[]>('remember_lut_files', { paths: selectedLutPaths });
        if (active) {
          await loadLibrary();
        }
      } catch (err) {
        console.error('Failed to sync LUT selection:', err);
        if (active) {
          setError(err instanceof Error ? err.message : '同步 LUT 资料失败');
        }
      }
    };

    void syncSelection();
    return () => {
      active = false;
    };
  }, [loadLibrary, selectedLutPaths]);

  useEffect(() => {
    const availablePaths = new Set([
      ...selectedLutPaths,
      ...libraryItems.map(item => item.path)
    ]);

    if (selectedPreviewPath && availablePaths.has(selectedPreviewPath)) {
      return;
    }

    setSelectedPreviewPath(selectedLutPaths[0] ?? libraryItems[0]?.path ?? null);
  }, [libraryItems, selectedLutPaths, selectedPreviewPath]);

  useEffect(() => {
    if (!isTauriEnv() || !selectedPreviewPath) {
      setPreviewSrc('');
      return;
    }

    let active = true;
    const loadPreview = async () => {
      try {
        setPreviewLoading(true);
        const previewPath = await invoke<string>('generate_lut_preview', {
          request: {
            lut_path: selectedPreviewPath,
            video_path: activeVideoPath ?? null,
            intensity: 1.0
          }
        });
        if (!active) return;
        setPreviewSrc(convertFileSrc(previewPath));
      } catch (err) {
        if (!active) return;
        console.error('Failed to generate LUT preview:', err);
        setPreviewSrc('');
      } finally {
        if (active) {
          setPreviewLoading(false);
        }
      }
    };

    void loadPreview();
    return () => {
      active = false;
    };
  }, [activeVideoPath, selectedPreviewPath]);

  const toggleSelected = useCallback((path: string) => {
    if (disabled) return;
    if (selectedSet.has(path)) {
      onSelectedLutPathsChange(selectedLutPaths.filter(item => item !== path));
      return;
    }
    onSelectedLutPathsChange([...selectedLutPaths, path]);
  }, [disabled, onSelectedLutPathsChange, selectedLutPaths, selectedSet]);

  const handleImportDirectory = useCallback(async () => {
    if (!isTauriEnv() || disabled || importing) return;

    try {
      setImporting(true);
      const selected = await open({ directory: true, multiple: false });
      if (!selected || Array.isArray(selected)) return;
      await invoke<LutLibraryItem[]>('import_lut_directory', { directory: selected });
      await loadLibrary();
    } catch (err) {
      console.error('Failed to import LUT directory:', err);
      setError(err instanceof Error ? err.message : '导入 LUT 目录失败');
    } finally {
      setImporting(false);
    }
  }, [disabled, importing, loadLibrary]);

  const handleRemove = useCallback(async (path: string) => {
    if (!isTauriEnv() || disabled) return;
    try {
      await invoke('remove_lut_from_library', { lutPath: path });
      if (selectedSet.has(path)) {
        onSelectedLutPathsChange(selectedLutPaths.filter(item => item !== path));
      }
      await loadLibrary();
    } catch (err) {
      console.error('Failed to remove LUT from library:', err);
      setError(err instanceof Error ? err.message : '移除 LUT 失败');
    }
  }, [disabled, loadLibrary, onSelectedLutPathsChange, selectedLutPaths, selectedSet]);

  const previewItem = useMemo(
    () => libraryItems.find(item => item.path === selectedPreviewPath) ?? selectedItems.find(item => item.path === selectedPreviewPath) ?? null,
    [libraryItems, selectedItems, selectedPreviewPath]
  );

  const batchConvertState = useMemo(() => {
    if (selectedItems.length === 0) {
      return { availableFormats: [] as Array<{ value: string; label: string }>, error: '请先选择 LUT 文件' };
    }

    if (selectedItems.some(item => item.is_placeholder)) {
      return { availableFormats: [] as Array<{ value: string; label: string }>, error: '请等待 LUT 资料同步完成后再转换' };
    }

    if (selectedItems.some(item => !item.is_valid)) {
      return { availableFormats: [] as Array<{ value: string; label: string }>, error: '仅支持转换可用的 LUT 文件' };
    }

    const dimensions = new Set(selectedItems.map(item => item.lut_type));
    if (dimensions.size > 1) {
      return {
        availableFormats: [] as Array<{ value: string; label: string }>,
        error: '当前选中项包含不同维度的 LUT，无法批量转换到同一目标格式'
      };
    }

    const availableKeys = selectedItems
      .map(item => SUPPORTED_TARGETS_BY_FORMAT[item.format] ?? [])
      .reduce<string[]>((acc, current, index) => {
        if (index === 0) return current;
        return acc.filter(value => current.includes(value));
      }, []);

    const availableFormats = availableKeys
      .map(key => FORMAT_VARIANTS[key])
      .filter(Boolean);

    if (availableFormats.length === 0) {
      return {
        availableFormats,
        error: '当前选中项没有可用的目标格式'
      };
    }

    return { availableFormats, error: null as string | null };
  }, [selectedItems]);

  const openBatchConvert = useCallback(() => {
    setBatchConvertTargetFormat('');
    setBatchConvertResult(null);
    setIsBatchConvertOpen(true);
  }, []);

  const handleBatchConvert = useCallback(async () => {
    if (!isTauriEnv() || !batchConvertTargetFormat || batchConvertState.error || selectedLutPaths.length === 0) {
      return;
    }

    try {
      setIsBatchConverting(true);
      const response = await invoke<BatchConvertResult>('batch_convert_luts', {
        request: {
          paths: selectedLutPaths,
          target_format: batchConvertTargetFormat
        }
      });
      setBatchConvertResult(response);
      await loadLibrary();
    } catch (err) {
      setError(err instanceof Error ? err.message : '批量转换失败');
    } finally {
      setIsBatchConverting(false);
    }
  }, [batchConvertState.error, batchConvertTargetFormat, loadLibrary, selectedLutPaths]);

  return (
    <div className="lut-library-panel">
      <div className="lut-library-header">
        <div>
          <h2>LUT 资料库</h2>
          <p>预览、复用和管理已导入的 LUT 文件</p>
        </div>
        <div className="lut-library-actions" style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
          <button
            className="lut-library-button secondary"
            onClick={() => void loadLibrary()}
            disabled={disabled || libraryLoading}
          >
            刷新
          </button>
          <button
            className="lut-library-button"
            onClick={() => void handleImportDirectory()}
            disabled={disabled || importing}
          >
            {importing ? '导入中...' : '导入目录'}
          </button>
          <button
            className="lut-library-button"
            onClick={openBatchConvert}
            disabled={disabled || selectedItems.length === 0}
          >
            批量转换
          </button>
          {onClose && (
            <button
              className="btn-close"
              type="button"
              onClick={onClose}
              aria-label="关闭"
              style={{ marginLeft: '8px' }}
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none">
                <path d="M18 6L6 18M6 6l12 12" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
              </svg>
            </button>
          )}
        </div>
      </div>

      {error && <div className="lut-library-error">{error}</div>}

      <div className="lut-library-preview">
        <div className="lut-preview-media">
          {previewLoading && <div className="lut-preview-placeholder">正在生成预览...</div>}
          {!previewLoading && previewSrc && (
            <img src={previewSrc} alt="LUT preview" className="lut-preview-image" />
          )}
          {!previewLoading && !previewSrc && (
            <div className="lut-preview-placeholder">
              {selectedPreviewPath ? '当前 LUT 暂无可用预览' : '选择一个 LUT 查看预览'}
            </div>
          )}
        </div>

        {previewItem && (
          <div className="lut-preview-meta">
            <div className="lut-preview-title">{previewItem.name}</div>
            <div className="lut-preview-tags">
              <span>{previewItem.category}</span>
              <span>{previewItem.format || 'UNKNOWN'}</span>
              <span>
                {previewItem.is_placeholder
                  ? '资料同步中'
                  : previewItem.is_valid
                    ? '可用'
                    : '无效'}
              </span>
            </div>
            <div className="lut-preview-details">
              <span>
                {previewItem.is_placeholder
                  ? '正在读取文件资料'
                  : previewItem.size > 0
                    ? formatFileSize(previewItem.size)
                    : '大小未知'}
              </span>
              {previewItem.updated_at && <span>{formatUpdatedAt(previewItem.updated_at)}</span>}
            </div>
            {previewItem.error_message && (
              <div className="lut-preview-warning">{previewItem.error_message}</div>
            )}
          </div>
        )}
      </div>

      {selectedItems.length > 0 && (
        <div className="lut-library-group">
          <div className="lut-library-section-title">当前任务</div>
          <div className="lut-library-list">
            {selectedItems.map(item => (
              <button
                key={`selected-${item.path}`}
                className={`lut-library-item ${selectedPreviewPath === item.path ? 'active' : ''}`}
                onClick={() => setSelectedPreviewPath(item.path)}
                type="button"
              >
                <div className="lut-library-item-main">
                  <div className="lut-library-item-name">{item.name}</div>
                  <div className="lut-library-item-meta">
                    <span>{item.category}</span>
                    <span>{item.format || 'UNKNOWN'}</span>
                    <span>
                      {item.is_placeholder
                        ? '同步中'
                        : item.is_valid
                          ? '已选中'
                          : '待修复'}
                    </span>
                  </div>
                </div>
                <div className="lut-library-item-actions">
                  <span className="lut-library-pill selected">已选</span>
                </div>
              </button>
            ))}
          </div>
        </div>
      )}

      <div className="lut-library-group">
        <div className="lut-library-section-title">资料库</div>
        <div className="lut-library-list">
          {libraryLoading && libraryItems.length === 0 && (
            <div className="lut-library-empty">正在加载资料库...</div>
          )}

          {!libraryLoading && libraryItems.length === 0 && (
            <div className="lut-library-empty">资料库还是空的，可以先从目录导入或直接选择 LUT 文件。</div>
          )}

          {libraryItems.map(item => (
            <div
              key={item.path}
              className={`lut-library-item ${selectedPreviewPath === item.path ? 'active' : ''}`}
            >
              <button
                type="button"
                className="lut-library-item-main"
                onClick={() => setSelectedPreviewPath(item.path)}
              >
                <div className="lut-library-item-name">{item.name}</div>
                <div className="lut-library-item-meta">
                  <span>{item.category}</span>
                  <span>{item.format || 'UNKNOWN'}</span>
                  <span>{item.is_valid ? '可用' : '无效'}</span>
                </div>
                {item.error_message && (
                  <div className="lut-library-item-error">{item.error_message}</div>
                )}
              </button>

              <div className="lut-library-item-actions">
                <button
                  className={`lut-library-pill ${selectedSet.has(item.path) ? 'selected' : ''}`}
                  onClick={() => toggleSelected(item.path)}
                  type="button"
                  disabled={disabled}
                >
                  {selectedSet.has(item.path) ? '移出当前' : '加入当前'}
                </button>
                <button
                  className="lut-library-icon"
                  onClick={() => void handleRemove(item.path)}
                  type="button"
                  disabled={disabled}
                  title="从资料库移除"
                >
                  ×
                </button>
              </div>
            </div>
          ))}
        </div>
      </div>

      {isBatchConvertOpen && (
        <div className="lut-batch-convert-overlay" onClick={() => setIsBatchConvertOpen(false)}>
          <div className="lut-batch-convert-dialog" onClick={(event) => event.stopPropagation()}>
            <div className="lut-batch-convert-header">
              <h3>批量转换 LUT</h3>
              <button
                type="button"
                className="btn-close"
                aria-label="关闭批量转换"
                onClick={() => setIsBatchConvertOpen(false)}
              >
                ×
              </button>
            </div>

            <div className="lut-batch-convert-body">
              <p className="lut-batch-convert-summary">{`已选 ${selectedItems.length} 个文件`}</p>

              {batchConvertState.error ? (
                <div className="lut-library-error">{batchConvertState.error}</div>
              ) : (
                <label className="lut-batch-convert-field">
                  <span>目标格式</span>
                  <select
                    aria-label="目标格式"
                    value={batchConvertTargetFormat}
                    onChange={(event) => setBatchConvertTargetFormat(event.target.value)}
                  >
                    <option value="">请选择</option>
                    {batchConvertState.availableFormats.map(format => (
                      <option key={format.value} value={format.value}>
                        {format.label}
                      </option>
                    ))}
                  </select>
                </label>
              )}

              <div className="lut-batch-convert-actions">
                <button
                  type="button"
                  className="lut-library-button secondary"
                  onClick={() => setIsBatchConvertOpen(false)}
                >
                  取消
                </button>
                <button
                  type="button"
                  className="lut-library-button"
                  disabled={Boolean(batchConvertState.error) || !batchConvertTargetFormat || isBatchConverting}
                  onClick={() => void handleBatchConvert()}
                >
                  {isBatchConverting ? '转换中...' : '开始转换'}
                </button>
              </div>

              {batchConvertResult && (
                <div className="lut-batch-convert-result">
                  <div className="lut-batch-convert-result-title">
                    {`成功 ${batchConvertResult.success_count} 个，失败 ${batchConvertResult.failure_count} 个`}
                  </div>
                  <div className="lut-batch-convert-result-list">
                    {batchConvertResult.results.map((item) => (
                      <div key={`${item.source_path}-${item.target_path ?? 'failed'}`} className="lut-batch-convert-result-item">
                        <div className="lut-batch-convert-result-source">{item.source_path}</div>
                        {item.target_path && (
                          <div className="lut-batch-convert-result-target">{item.target_path}</div>
                        )}
                        {item.error_message && (
                          <div className="lut-library-item-error">{item.error_message}</div>
                        )}
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default LutLibraryPanel;
