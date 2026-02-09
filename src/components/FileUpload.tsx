import React, { useCallback, useEffect, useState, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import './FileUpload.css';

interface FileUploadProps {
  onVideoSelect: (filePaths: string[]) => void;
  onActiveVideoChange: (filePath: string | null) => void;
  onLutSelect: (filePaths: string[]) => void;
  lutPaths?: string[];
  disabled?: boolean;
}

interface FileInfo {
  name: string;
  path: string;
  size?: number;
  type?: string;
}

const FileUpload: React.FC<FileUploadProps> = ({
  onVideoSelect,
  onActiveVideoChange,
  onLutSelect,
  lutPaths = [],
  disabled = false
}) => {
  const [videoFiles, setVideoFiles] = useState<FileInfo[]>([]);
  const [activeVideoPath, setActiveVideoPath] = useState<string | null>(null);
  const [lutFiles, setLutFiles] = useState<FileInfo[]>([]);
  const [dragOver, setDragOver] = useState<'video' | 'lut' | null>(null);
  const [loading, setLoading] = useState(false);

  // Fallback: hidden inputs for browser env
  const videoInputRef = useRef<HTMLInputElement | null>(null);
  const lutInputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    setLutFiles(prev => {
      const current = prev.map(x => x.path);
      const sameLength = current.length === lutPaths.length;
      const sameOrder = sameLength && current.every((p, i) => p === lutPaths[i]);
      if (sameOrder) return prev;

      const byPath = new Map(prev.map(x => [x.path, x] as const));
      return lutPaths.map(path => {
        const existing = byPath.get(path);
        if (existing) return existing;
        const name = path.split('/').pop() || path.split('\\').pop() || 'Unknown';
        return { name, path } satisfies FileInfo;
      });
    });
  }, [lutPaths]);

  const handleVideoInputChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(e.target.files || []);
    if (files.length === 0) return;

    const selectedInfos: FileInfo[] = files.map(file => ({
      name: file.name,
      path: (file as any).path || file.name,
      size: file.size,
      type: file.type
    }));

    setVideoFiles(prev => {
      const next = [...prev];
      for (const info of selectedInfos) {
        if (!next.some(x => x.path === info.path)) next.push(info);
      }

      const nextActive = activeVideoPath ?? (next[0]?.path ?? null);
      setActiveVideoPath(nextActive);
      onActiveVideoChange(nextActive);
      onVideoSelect(next.map(x => x.path));
      return next;
    });
    // reset input so selecting the same file again still triggers change
    e.target.value = '';
  }, [activeVideoPath, onActiveVideoChange, onVideoSelect]);

  const handleLutInputChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(e.target.files || []);
    if (files.length === 0) return;

    const selectedInfos: FileInfo[] = files.map(file => ({
      name: file.name,
      path: (file as any).path || file.name,
      size: file.size,
      type: file.type
    }));

    setLutFiles(prev => {
      const next = [...prev];
      for (const info of selectedInfos) {
        if (!next.some(x => x.path === info.path)) next.push(info);
      }
      onLutSelect(next.map(x => x.path));
      return next;
    });
    e.target.value = '';
  }, [onLutSelect]);

  // 选择视频文件
  const selectVideoFile = useCallback(async () => {
    if (disabled || loading) return;

    try {
      setLoading(true);
      const selected = await open({
        multiple: true,
        filters: [{
          name: 'Video Files',
          extensions: ['mp4', 'mov', 'avi', 'mkv', 'wmv', 'flv', 'webm', 'm4v']
        }]
      });

      if (selected) {
        const filePaths = Array.isArray(selected) ? selected : [selected];
        const infos = await Promise.all(
          filePaths.map(async (filePath) => {
            const fileName = filePath.split('/').pop() || filePath.split('\\').pop() || 'Unknown';
            try {
              const fileInfo = await invoke<{ size: number; type: string }>('get_file_info', { path: filePath });
              return {
                name: fileName,
                path: filePath,
                size: fileInfo.size,
                type: fileInfo.type
              } satisfies FileInfo;
            } catch (error) {
              console.error('Failed to get file info:', error);
              return { name: fileName, path: filePath } satisfies FileInfo;
            }
          })
        );

        setVideoFiles(prev => {
          const next = [...prev];
          for (const info of infos) {
            if (!next.some(x => x.path === info.path)) next.push(info);
          }

          const nextActive = activeVideoPath ?? (next[0]?.path ?? null);
          setActiveVideoPath(nextActive);
          onActiveVideoChange(nextActive);
          onVideoSelect(next.map(x => x.path));
          return next;
        });
      } else {
        // user cancelled or not available, try fallback input
        videoInputRef.current?.click();
      }
    } catch (error) {
      console.error('Failed to select video file:', error);
      // Fallback to HTML input in non-Tauri environments
      videoInputRef.current?.click();
    } finally {
      setLoading(false);
    }
  }, [activeVideoPath, disabled, loading, onActiveVideoChange, onVideoSelect]);

  // 选择LUT文件
  const selectLutFile = useCallback(async () => {
    if (disabled || loading) return;

    try {
      setLoading(true);
      const selected = await open({
        multiple: true,
        filters: [{
          name: 'LUT Files',
          extensions: ['cube', '3dl', 'lut', 'csp', 'vlt', 'mga', 'm3d', 'look']
        }]
      });

      if (selected) {
        const filePaths = Array.isArray(selected) ? selected : [selected];
        const infos = await Promise.all(
          filePaths.map(async (filePath) => {
            const fileName = filePath.split('/').pop() || filePath.split('\\').pop() || 'Unknown';
            try {
              const fileInfo = await invoke<{ size: number; type: string }>('get_file_info', { path: filePath });
              return {
                name: fileName,
                path: filePath,
                size: fileInfo.size,
                type: fileInfo.type
              } satisfies FileInfo;
            } catch (error) {
              console.error('Failed to get file info:', error);
              return { name: fileName, path: filePath } satisfies FileInfo;
            }
          })
        );

        setLutFiles(prev => {
          const next = [...prev];
          for (const info of infos) {
            if (!next.some(x => x.path === info.path)) next.push(info);
          }
          onLutSelect(next.map(x => x.path));
          return next;
        });
      } else {
        lutInputRef.current?.click();
      }
    } catch (error) {
      console.error('Failed to select LUT file:', error);
      lutInputRef.current?.click();
    } finally {
      setLoading(false);
    }
  }, [disabled, loading, onLutSelect]);

  // 拖拽处理
  const handleDragOver = useCallback((e: React.DragEvent, type: 'video' | 'lut') => {
    e.preventDefault();
    if (!disabled && !loading) {
      setDragOver(type);
    }
  }, [disabled, loading]);

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    setDragOver(null);
  }, []);

  const handleDrop = useCallback(async (e: React.DragEvent, type: 'video' | 'lut') => {
    e.preventDefault();
    setDragOver(null);
    
    if (disabled || loading) return;

    const files = Array.from(e.dataTransfer.files);
    if (files.length === 0) return;

    if (type === 'video') {
      const videoExtensions = ['mp4', 'mov', 'avi', 'mkv', 'wmv', 'flv', 'webm', 'm4v'];
      const selectedInfos: FileInfo[] = [];
      for (const file of files) {
        const extension = file.name.split('.').pop()?.toLowerCase();
        if (!extension || !videoExtensions.includes(extension)) continue;
        selectedInfos.push({
          name: file.name,
          path: (file as any).path || file.name,
          size: file.size,
          type: file.type
        });
      }
      if (selectedInfos.length > 0) {
        setVideoFiles(prev => {
          const next = [...prev];
          for (const info of selectedInfos) {
            if (!next.some(x => x.path === info.path)) next.push(info);
          }
          const nextActive = activeVideoPath ?? (next[0]?.path ?? null);
          setActiveVideoPath(nextActive);
          onActiveVideoChange(nextActive);
          onVideoSelect(next.map(x => x.path));
          return next;
        });
      }
    } else if (type === 'lut') {
      const lutExtensions = ['cube', '3dl', 'lut', 'csp', 'vlt', 'mga', 'm3d', 'look'];
      const selectedInfos: FileInfo[] = [];
      for (const file of files) {
        const extension = file.name.split('.').pop()?.toLowerCase();
        if (!extension || !lutExtensions.includes(extension)) continue;
        selectedInfos.push({
          name: file.name,
          path: (file as any).path || file.name,
          size: file.size,
          type: file.type
        });
      }
      if (selectedInfos.length > 0) {
        setLutFiles(prev => {
          const next = [...prev];
          for (const info of selectedInfos) {
            if (!next.some(x => x.path === info.path)) next.push(info);
          }
          onLutSelect(next.map(x => x.path));
          return next;
        });
      }
    }
  }, [activeVideoPath, disabled, loading, onActiveVideoChange, onVideoSelect, onLutSelect]);

  // 格式化文件大小
  const formatFileSize = (bytes?: number): string => {
    if (!bytes) return '';
    
    const units = ['B', 'KB', 'MB', 'GB'];
    let size = bytes;
    let unitIndex = 0;
    
    while (size >= 1024 && unitIndex < units.length - 1) {
      size /= 1024;
      unitIndex++;
    }
    
    return `${size.toFixed(1)} ${units[unitIndex]}`;
  };

  // 清除文件
  const removeVideoAt = (index: number) => {
    setVideoFiles(prev => {
      const removed = prev[index];
      const next = prev.filter((_, i) => i !== index);
      const nextActive =
        removed?.path && activeVideoPath === removed.path ? (next[0]?.path ?? null) : activeVideoPath;
      setActiveVideoPath(nextActive);
      onActiveVideoChange(nextActive);
      onVideoSelect(next.map(x => x.path));
      return next;
    });
  };

  const selectActiveVideo = (path: string) => {
    setActiveVideoPath(path);
    onActiveVideoChange(path);
  };

  const removeLutAt = (index: number) => {
    setLutFiles(prev => {
      const next = prev.filter((_, i) => i !== index);
      onLutSelect(next.map(x => x.path));
      return next;
    });
  };

  const moveLut = (from: number, to: number) => {
    setLutFiles(prev => {
      if (to < 0 || to >= prev.length) return prev;
      const next = [...prev];
      const [item] = next.splice(from, 1);
      next.splice(to, 0, item);
      onLutSelect(next.map(x => x.path));
      return next;
    });
  };

  return (
    <div className="file-upload">
      {/* Hidden inputs for browser fallback */}
      <input
        ref={videoInputRef}
        type="file"
        accept=".mp4,.mov,.avi,.mkv,.wmv,.flv,.webm,.m4v"
        style={{ display: 'none' }}
        multiple
        onChange={handleVideoInputChange}
      />
      <input
        ref={lutInputRef}
        type="file"
        accept=".cube,.3dl,.lut,.csp,.vlt,.mga,.m3d,.look"
        style={{ display: 'none' }}
        multiple
        onChange={handleLutInputChange}
      />

      <div className="upload-section">
        <h3>选择文件</h3>
        
        {/* 视频文件上传区域 */}
        <div 
          className={`upload-area video-upload-area ${
            dragOver === 'video' ? 'drag-over' : ''
          } ${disabled ? 'disabled' : ''}`}
          onDragOver={(e) => handleDragOver(e, 'video')}
          onDragLeave={handleDragLeave}
          onDrop={(e) => handleDrop(e, 'video')}
          onClick={selectVideoFile}
        >
          <div className="upload-content">
            {videoFiles.length > 0 ? (
              <div className="video-list">
                {videoFiles.map((videoFile, index) => (
                  <div
                    className={`video-item ${activeVideoPath === videoFile.path ? 'active' : ''}`}
                    key={videoFile.path}
                    onClick={(e) => {
                      e.stopPropagation();
                      selectActiveVideo(videoFile.path);
                    }}
                    role="button"
                    tabIndex={0}
                  >
                    <div className="file-icon video-icon">🎬</div>
                    <div className="file-details">
                      <div className="file-name">{videoFile.name}</div>
                      <div className="file-meta">
                        {videoFile.size && (
                          <span className="file-size">{formatFileSize(videoFile.size)}</span>
                        )}
                        {activeVideoPath === videoFile.path && (
                          <span className="badge">预览中</span>
                        )}
                      </div>
                    </div>
                    <div className="video-actions">
                      <button
                        className="icon-button"
                        onClick={(e) => {
                          e.stopPropagation();
                          selectActiveVideo(videoFile.path);
                        }}
                        disabled={disabled || activeVideoPath === videoFile.path}
                        aria-label="设为预览"
                      >
                        ▶
                      </button>
                      <button
                        className="clear-button"
                        onClick={(e) => {
                          e.stopPropagation();
                          removeVideoAt(index);
                        }}
                        disabled={disabled}
                        aria-label="移除"
                      >
                        ✕
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <div className="upload-placeholder">
                <div className="upload-icon">📹</div>
                <div className="upload-text">
                  <div className="primary-text">选择视频文件</div>
                  <div className="secondary-text">
                    支持 MP4, MOV, AVI, MKV 等格式
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>

        {/* LUT 文件上传区域 */}
        <div 
          className={`upload-area lut-upload-area ${
            dragOver === 'lut' ? 'drag-over' : ''
          } ${disabled ? 'disabled' : ''}`}
          onDragOver={(e) => handleDragOver(e, 'lut')}
          onDragLeave={handleDragLeave}
          onDrop={(e) => handleDrop(e, 'lut')}
          onClick={selectLutFile}
        >
          <div className="upload-content">
            {lutFiles.length > 0 ? (
              <div className="lut-list">
                {lutFiles.map((lutFile, index) => (
                  <div className="lut-item" key={lutFile.path}>
                    <div className="file-icon lut-icon">🎨</div>
                    <div className="file-details">
                      <div className="file-name">{lutFile.name}</div>
                      <div className="file-meta">
                        {lutFile.size && (
                          <span className="file-size">{formatFileSize(lutFile.size)}</span>
                        )}
                      </div>
                    </div>
                    <div className="lut-actions">
                      <button
                        className="icon-button"
                        onClick={(e) => {
                          e.stopPropagation();
                          moveLut(index, index - 1);
                        }}
                        disabled={disabled || index === 0}
                        aria-label="上移"
                      >
                        ↑
                      </button>
                      <button
                        className="icon-button"
                        onClick={(e) => {
                          e.stopPropagation();
                          moveLut(index, index + 1);
                        }}
                        disabled={disabled || index === lutFiles.length - 1}
                        aria-label="下移"
                      >
                        ↓
                      </button>
                      <button
                        className="clear-button"
                        onClick={(e) => {
                          e.stopPropagation();
                          removeLutAt(index);
                        }}
                        disabled={disabled}
                        aria-label="移除"
                      >
                        ✕
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <div className="upload-placeholder">
                <div className="upload-icon">📁</div>
                <div className="upload-text">
                  <div className="primary-text">选择LUT文件</div>
                  <div className="secondary-text">
                    支持 .cube, .3dl, .lut 等格式
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

export default FileUpload;
