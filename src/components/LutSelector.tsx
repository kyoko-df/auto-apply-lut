import React, { useCallback, useMemo, useRef, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { Palette, X } from 'lucide-react';

interface LutSelectorProps {
  title?: string;
  disabled?: boolean;
  onSelect: (path: string | null) => void;
}

const LUT_EXTENSIONS = ['cube', '3dl', 'lut', 'csp', 'vlt', 'mga', 'm3d', 'look'];

const LutSelector: React.FC<LutSelectorProps> = ({
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
    <div className="w-full">
      <input
        ref={inputRef}
        type="file"
        accept={acceptAttr}
        className="hidden"
        onChange={handleInput}
      />

      <div
        className={`
          relative border border-dashed rounded-lg p-3 transition-all duration-200 ease-apple cursor-pointer
          ${dragOver
            ? 'border-[var(--color-accent)] bg-[var(--color-accent-subtle)]'
            : 'border-[var(--color-border)] hover:border-[var(--color-text-secondary)] bg-[var(--color-surface)]'
          }
          ${disabled ? 'opacity-50 cursor-not-allowed' : ''}
        `}
        onDragOver={onDragOver}
        onDragLeave={onDragLeave}
        onDrop={onDrop}
        onClick={pickLut}
      >
        {lutPath ? (
          <div className="flex items-center gap-2">
            <div className="p-1.5 rounded-md bg-[var(--color-accent)] text-white">
              <Palette size={14} />
            </div>
            <div className="flex-1 min-w-0">
              <div className="text-xs font-medium text-[var(--color-text-primary)] truncate">
                {lutPath.split(/[/\\]/).pop() || lutPath}
              </div>
              <div className="text-[10px] text-[var(--color-text-tertiary)] truncate">
                {lutPath}
              </div>
            </div>
            <button
              onClick={(e) => { e.stopPropagation(); clear(); }}
              className="p-1 rounded-full hover:bg-[var(--color-background)] text-[var(--color-text-tertiary)] hover:text-[var(--color-danger)] transition-colors"
              disabled={disabled}
            >
              <X size={12} />
            </button>
          </div>
        ) : (
          <div className="flex flex-col items-center justify-center text-center py-1">
            <div className="flex items-center gap-2 text-[var(--color-text-secondary)]">
              <Palette size={14} />
              <span className="text-xs font-medium">选择 LUT 文件</span>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

export default LutSelector;