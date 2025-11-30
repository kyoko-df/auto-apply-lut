import React, { useCallback, useMemo, useRef, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { Upload, FileVideo, X } from 'lucide-react';

interface MultiFileSelectorProps {
  title?: string;
  acceptExtensions?: string[];
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
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const extensionsSet = useMemo(() => new Set(acceptExtensions.map(e => e.toLowerCase())), [acceptExtensions]);

  const notifyChange = useCallback((next: string[]) => {
    setSelectedPaths(next);
    onChange(next);
  }, [onChange]);

  const dedupeMerge = useCallback((prev: string[], next: string[]) => {
    const merged = [...prev, ...next];
    return Array.from(new Set(merged.map(p => p.trim())));
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

      if (selected) {
        const paths = Array.isArray(selected) ? selected : [selected];
        const next = dedupeMerge(selectedPaths, paths);
        notifyChange(next);
      } else {
        fileInputRef.current?.click();
      }
    } catch (e) {
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

  const acceptAttr = useMemo(() => '.' + Array.from(extensionsSet).join(',.'), [extensionsSet]);

  return (
    <div className="w-full group/selector">
      <input
        ref={fileInputRef}
        type="file"
        multiple
        accept={acceptAttr}
        className="hidden"
        onChange={handleInputChange}
      />

      {title && <div className="mb-2 text-sm font-medium text-[var(--color-text-secondary)]">{title}</div>}

      <div
        className={`
          relative border rounded-xl p-5 transition-all duration-300 ease-apple cursor-pointer
          ${dragOver
            ? 'border-[var(--color-accent)] bg-[var(--color-accent-subtle)] shadow-[0_0_0_4px_var(--color-accent-subtle)]'
            : 'border-dashed border-[var(--color-border)] hover:border-[var(--color-accent)] bg-[var(--color-surface)] hover:bg-[var(--color-surface-translucent)]'
          }
          ${disabled ? 'opacity-50 cursor-not-allowed' : 'active:scale-[0.99]'}
        `}
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
        onClick={pickFiles}
      >
        <div className="flex flex-col items-center justify-center text-center py-2">
          <div className={`
            p-3 rounded-full mb-3 transition-colors duration-300
            ${dragOver ? 'bg-[var(--color-accent)] text-white' : 'bg-[var(--color-background)] text-[var(--color-text-secondary)] group-hover/selector:text-[var(--color-accent)] group-hover/selector:bg-[var(--color-accent-subtle)]'}
          `}>
            <Upload size={20} strokeWidth={2} />
          </div>
          <p className="text-sm font-medium text-[var(--color-text-primary)] mb-1">点击或拖拽上传</p>
          <p className="text-[11px] text-[var(--color-text-tertiary)]">支持 MP4, MOV 等格式</p>
        </div>
      </div>

      {selectedPaths.length > 0 && (
        <div className="mt-3 space-y-2 max-h-[240px] overflow-y-auto pr-1 custom-scrollbar">
          {selectedPaths.map((p) => {
            const name = p.split(/[/\\]/).pop() || p;
            return (
              <div key={p} className="group flex items-center gap-3 p-2.5 rounded-lg bg-[var(--color-surface)] border border-[var(--color-border)] hover:border-[var(--color-accent-subtle)] hover:shadow-sm transition-all">
                <div className="p-1.5 rounded-md bg-[var(--color-background)] text-[var(--color-text-secondary)]">
                  <FileVideo size={16} strokeWidth={2} />
                </div>
                <div className="flex-1 min-w-0">
                  <div className="text-xs font-medium text-[var(--color-text-primary)] truncate" title={name}>{name}</div>
                  <div className="text-[10px] text-[var(--color-text-tertiary)] truncate opacity-80" title={p}>{p}</div>
                </div>
                <button
                  onClick={(e) => { e.stopPropagation(); removeItem(p); }}
                  className="p-1.5 rounded-md hover:bg-[var(--color-danger)] hover:text-white text-[var(--color-text-tertiary)] transition-colors opacity-0 group-hover:opacity-100 focus:opacity-100"
                  title="移除"
                >
                  <X size={14} />
                </button>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
};

export default MultiFileSelector;