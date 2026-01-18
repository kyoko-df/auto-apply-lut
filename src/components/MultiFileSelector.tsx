import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import './FileUpload.css';

interface MultiFileSelectorProps {
  title?: string;
  acceptExtensions?: string[]; // e.g. ['mp4','mov']
  disabled?: boolean;
  onChange: (paths: string[]) => void;
}

const DEFAULT_VIDEO_EXTENSIONS = ['mp4', 'mov', 'avi', 'mkv', 'wmv', 'flv', 'webm', 'm4v'];

const MultiFileSelector: React.FC<MultiFileSelectorProps> = ({
  title = '批量选择视频文件',
  acceptExtensions = DEFAULT_VIDEO_EXTENSIONS,
  disabled = false,
  onChange,
}) => {
  const [selectedPaths, setSelectedPaths] = useState<string[]>([]);
  const [dragOver, setDragOver] = useState(false);
  const [loading, setLoading] = useState(false);

  // Browser fallback input
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const extensionsSet = useMemo(() => new Set(acceptExtensions.map(e => e.toLowerCase())), [acceptExtensions]);

  const notifyChange = useCallback((next: string[]) => {
    setSelectedPaths(next);
    onChange(next);
  }, [onChange]);

  const dedupeMerge = useCallback((prev: string[], next: string[]) => {
    const merged = [...prev, ...next];
    const uniq = Array.from(new Set(merged.map(p => p.trim())));
    return uniq;
  }, []);

  const pickFiles = useCallback(async () => {
    if (disabled || loading) return;
    try {
      setLoading(true);
      const selected = await open({
        multiple: true,
        directory: false,
        filters: [{ name: 'Video Files', extensions: acceptExtensions }],
      });

      if (selected && Array.isArray(selected)) {
        const next = dedupeMerge(selectedPaths, selected);
        notifyChange(next);
      } else if (typeof selected === 'string') {
        const next = dedupeMerge(selectedPaths, [selected]);
        notifyChange(next);
      } else {
        // User cancelled or not in Tauri env: fallback to browser input
        fileInputRef.current?.click();
      }
    } catch (e) {
      // Fallback in non-Tauri environments
      fileInputRef.current?.click();
    } finally {
      setLoading(false);
    }
  }, [acceptExtensions, dedupeMerge, disabled, loading, notifyChange, selectedPaths]);

  const handleInputChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(e.target.files ?? []);
    if (files.length === 0) return;
    const paths = files.map(f => (f as any).path || f.name);
    const next = dedupeMerge(selectedPaths, paths);
    notifyChange(next);
    e.target.value = '';
  }, [dedupeMerge, notifyChange, selectedPaths]);

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    if (!disabled && !loading) setDragOver(true);
  }, [disabled, loading]);

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    setDragOver(false);
  }, []);

  const handleDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    setDragOver(false);
    if (disabled || loading) return;

    const files = Array.from(e.dataTransfer.files);
    if (files.length === 0) return;
    const allowed = files.filter(f => {
      const ext = f.name.split('.').pop()?.toLowerCase();
      return ext ? extensionsSet.has(ext) : false;
    });
    if (allowed.length === 0) return;

    const paths = allowed.map(f => (f as any).path || f.name);
    const next = dedupeMerge(selectedPaths, paths);
    notifyChange(next);
  }, [dedupeMerge, disabled, extensionsSet, loading, notifyChange, selectedPaths]);

  const removeItem = useCallback((path: string) => {
    const next = selectedPaths.filter(p => p !== path);
    notifyChange(next);
  }, [notifyChange, selectedPaths]);

  const clearAll = useCallback(() => {
    notifyChange([]);
  }, [notifyChange]);

  // Compose accept string for input
  const acceptAttr = useMemo(() => '.' + Array.from(extensionsSet).join(',.'), [extensionsSet]);

  useEffect(() => {
    // Sync prop changes (acceptExtensions) won't reset existing selections
  }, [acceptExtensions]);

  return (
    <div className="file-upload" style={{ marginTop: 16 }}>
      {/* Hidden input as browser fallback */}
      <input
        ref={fileInputRef}
        type="file"
        multiple
        accept={acceptAttr}
        style={{ display: 'none' }}
        onChange={handleInputChange}
      />

      <div className="upload-section">
        <h3>{title}</h3>

        <div
          className={`upload-area ${dragOver ? 'drag-over' : ''} ${disabled ? 'disabled' : ''}`}
          onDragOver={handleDragOver}
          onDragLeave={handleDragLeave}
          onDrop={handleDrop}
          onClick={pickFiles}
        >
          <div className="upload-content">
            <div className="upload-placeholder">
              <div className="upload-icon">📁</div>
              <div className="upload-text">
                <div className="primary-text">点击或拖拽添加文件</div>
                <div className="secondary-text">支持 {acceptExtensions.join(', ').toUpperCase()} 等格式</div>
                <div className="hint-text">可多次选择，自动累积并去重</div>
              </div>
            </div>
          </div>
        </div>

        {selectedPaths.length > 0 && (
          <div style={{ marginTop: 12, display: 'flex', flexDirection: 'column', gap: 8 }}>
            {selectedPaths.map((p) => {
              const name = p.split('/').pop() || p.split('\\').pop() || p;
              return (
                <div key={p} className="file-info">
                  <div className="file-icon video-icon">🎬</div>
                  <div className="file-details">
                    <div className="file-name">{name}</div>
                    <div className="file-meta">
                      <span className="file-size" style={{ fontSize: '0.8rem' }}>{p}</span>
                    </div>
                  </div>
                  <button
                    className="clear-button"
                    onClick={(e) => { e.stopPropagation(); removeItem(p); }}
                    disabled={disabled}
                    aria-label="移除"
                  >
                    ✕
                  </button>
                </div>
              );
            })}

            <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
              <button
                onClick={() => fileInputRef.current?.click()}
                disabled={disabled || loading}
                style={{
                  padding: '8px 12px', borderRadius: 8, border: '1px solid var(--border-soft)',
                  background: 'var(--surface-card)', cursor: 'pointer'
                }}
              >
                通过浏览器选择
              </button>
              <button
                onClick={clearAll}
                disabled={disabled || loading}
                style={{
                  padding: '8px 12px', borderRadius: 8, border: 'none',
                  background: 'var(--color-danger)', color: 'white', cursor: 'pointer'
                }}
              >
                清空全部
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

export default MultiFileSelector;