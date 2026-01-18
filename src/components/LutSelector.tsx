import React, { useCallback, useMemo, useRef, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import './FileUpload.css';

interface LutSelectorProps {
  title?: string;
  disabled?: boolean;
  onSelect: (path: string | null) => void;
}

const LUT_EXTENSIONS = ['cube', '3dl', 'lut', 'csp', 'vlt', 'mga', 'm3d', 'look'];

const LutSelector: React.FC<LutSelectorProps> = ({
  title = '选择 LUT 文件',
  disabled = false,
  onSelect,
}) => {
  const [lutPath, setLutPath] = useState<string | null>(null);
  const [dragOver, setDragOver] = useState(false);
  const [loading, setLoading] = useState(false);
  const inputRef = useRef<HTMLInputElement | null>(null);

  const acceptAttr = useMemo(() => '.' + LUT_EXTENSIONS.join(',.'), []);

  const notify = useCallback((path: string | null) => {
    setLutPath(path);
    onSelect(path);
  }, [onSelect]);

  const pickLut = useCallback(async () => {
    if (disabled || loading) return;
    try {
      setLoading(true);
      const selected = await open({
        multiple: false,
        directory: false,
        filters: [{ name: 'LUT Files', extensions: LUT_EXTENSIONS }],
      });
      if (selected) {
        const path = Array.isArray(selected) ? selected[0] : selected;
        notify(path);
      } else {
        inputRef.current?.click();
      }
    } catch (e) {
      inputRef.current?.click();
    } finally {
      setLoading(false);
    }
  }, [disabled, loading, notify]);

  const handleInput = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(e.target.files ?? []);
    if (files.length === 0) return;
    const f = files[0];
    const path = (f as any).path || f.name;
    notify(path);
    e.target.value = '';
  }, [notify]);

  const onDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    if (!disabled && !loading) setDragOver(true);
  }, [disabled, loading]);

  const onDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    setDragOver(false);
  }, []);

  const onDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    setDragOver(false);
    if (disabled || loading) return;
    const files = Array.from(e.dataTransfer.files);
    if (files.length === 0) return;
    const f = files[0];
    const ext = f.name.split('.').pop()?.toLowerCase();
    if (!ext || !LUT_EXTENSIONS.includes(ext)) return;
    const path = (f as any).path || f.name;
    notify(path);
  }, [disabled, loading, notify]);

  const clear = useCallback(() => notify(null), [notify]);

  return (
    <div className="file-upload" style={{ marginTop: 16 }}>
      <input
        ref={inputRef}
        type="file"
        accept={acceptAttr}
        style={{ display: 'none' }}
        onChange={handleInput}
      />

      <div className="upload-section">
        <h3>{title}</h3>
        <div
          className={`upload-area ${dragOver ? 'drag-over' : ''} ${disabled ? 'disabled' : ''}`}
          onDragOver={onDragOver}
          onDragLeave={onDragLeave}
          onDrop={onDrop}
          onClick={pickLut}
        >
          <div className="upload-content">
            {lutPath ? (
              <div className="file-info">
                <div className="file-icon lut-icon">🎨</div>
                <div className="file-details">
                  <div className="file-name">{lutPath.split('/').pop() || lutPath.split('\\').pop() || lutPath}</div>
                  <div className="file-meta">
                    <span className="file-size" style={{ fontSize: '0.8rem' }}>{lutPath}</span>
                  </div>
                </div>
                <button
                  className="clear-button"
                  onClick={(e) => { e.stopPropagation(); clear(); }}
                  disabled={disabled}
                  aria-label="清除LUT"
                >
                  ✕
                </button>
              </div>
            ) : (
              <div className="upload-placeholder">
                <div className="upload-icon">🎨</div>
                <div className="upload-text">
                  <div className="primary-text">点击或拖拽选择 LUT 文件</div>
                  <div className="secondary-text">支持 {LUT_EXTENSIONS.join(', ').toUpperCase()}</div>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

export default LutSelector;