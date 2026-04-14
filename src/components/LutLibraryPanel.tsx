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
}

interface LutLibraryPanelProps {
  activeVideoPath?: string | null;
  selectedLutPaths: string[];
  onSelectedLutPathsChange: (paths: string[]) => void;
  disabled?: boolean;
}

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
  disabled = false
}) => {
  const [libraryItems, setLibraryItems] = useState<LutLibraryItem[]>([]);
  const [selectedPreviewPath, setSelectedPreviewPath] = useState<string | null>(null);
  const [previewSrc, setPreviewSrc] = useState<string>('');
  const [libraryLoading, setLibraryLoading] = useState(false);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [importing, setImporting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const selectedSet = useMemo(() => new Set(selectedLutPaths), [selectedLutPaths]);
  const selectedItems = useMemo(() => {
    const byPath = new Map(libraryItems.map(item => [item.path, item] as const));
    return selectedLutPaths.map(path => byPath.get(path) ?? {
      path,
      name: path.split(/[\\/]/).pop() || path,
      size: 0,
      lut_type: 'Unknown',
      format: path.split('.').pop()?.toUpperCase() || '',
      category: 'Selected',
      is_valid: true,
      updated_at: '',
      error_message: null
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

  return (
    <div className="lut-library-panel card">
      <div className="lut-library-header">
        <div>
          <h3>LUT 资料库</h3>
          <p>预览、复用和管理已导入的 LUT 文件</p>
        </div>
        <div className="lut-library-actions">
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
              <span>{previewItem.is_valid ? '可用' : '无效'}</span>
            </div>
            <div className="lut-preview-details">
              <span>{previewItem.size > 0 ? formatFileSize(previewItem.size) : '大小未知'}</span>
              {previewItem.updated_at && <span>{formatUpdatedAt(previewItem.updated_at)}</span>}
            </div>
            {previewItem.error_message && (
              <div className="lut-preview-warning">{previewItem.error_message}</div>
            )}
          </div>
        )}
      </div>

      {selectedItems.length > 0 && (
        <div className="lut-library-section">
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
                    <span>{item.is_valid ? '已选中' : '待修复'}</span>
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

      <div className="lut-library-section">
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
    </div>
  );
};

export default LutLibraryPanel;
