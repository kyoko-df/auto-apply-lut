import React, { useCallback, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import './FileUpload.css';

interface FileUploadProps {
  onVideoSelect: (filePath: string) => void;
  onLutSelect: (filePath: string) => void;
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
  onLutSelect,
  disabled = false
}) => {
  const [videoFile, setVideoFile] = useState<FileInfo | null>(null);
  const [lutFile, setLutFile] = useState<FileInfo | null>(null);
  const [dragOver, setDragOver] = useState<'video' | 'lut' | null>(null);
  const [loading, setLoading] = useState(false);

  // 选择视频文件
  const selectVideoFile = useCallback(async () => {
    if (disabled || loading) return;

    try {
      setLoading(true);
      const selected = await open({
        multiple: false,
        filters: [{
          name: 'Video Files',
          extensions: ['mp4', 'mov', 'avi', 'mkv', 'wmv', 'flv', 'webm', 'm4v']
        }]
      });

      if (selected) {
        const filePath = Array.isArray(selected) ? selected[0] : selected;
        const fileName = filePath.split('/').pop() || filePath.split('\\').pop() || 'Unknown';
        
        // 获取文件信息
        try {
          const fileInfo = await invoke<{ size: number; type: string }>('get_file_info', { path: filePath });
          const file: FileInfo = {
            name: fileName,
            path: filePath,
            size: fileInfo.size,
            type: fileInfo.type
          };
          
          setVideoFile(file);
          onVideoSelect(filePath);
        } catch (error) {
          console.error('Failed to get file info:', error);
          // 即使获取文件信息失败，也设置基本信息
          const file: FileInfo = {
            name: fileName,
            path: filePath
          };
          setVideoFile(file);
          onVideoSelect(filePath);
        }
      }
    } catch (error) {
      console.error('Failed to select video file:', error);
    } finally {
      setLoading(false);
    }
  }, [disabled, loading, onVideoSelect]);

  // 选择LUT文件
  const selectLutFile = useCallback(async () => {
    if (disabled || loading) return;

    try {
      setLoading(true);
      const selected = await open({
        multiple: false,
        filters: [{
          name: 'LUT Files',
          extensions: ['cube', '3dl', 'lut', 'csp', 'vlt', 'mga', 'm3d', 'look']
        }]
      });

      if (selected) {
        const filePath = Array.isArray(selected) ? selected[0] : selected;
        const fileName = filePath.split('/').pop() || filePath.split('\\').pop() || 'Unknown';
        
        // 获取文件信息
        try {
          const fileInfo = await invoke<{ size: number; type: string }>('get_file_info', { path: filePath });
          const file: FileInfo = {
            name: fileName,
            path: filePath,
            size: fileInfo.size,
            type: fileInfo.type
          };
          
          setLutFile(file);
          onLutSelect(filePath);
        } catch (error) {
          console.error('Failed to get file info:', error);
          // 即使获取文件信息失败，也设置基本信息
          const file: FileInfo = {
            name: fileName,
            path: filePath
          };
          setLutFile(file);
          onLutSelect(filePath);
        }
      }
    } catch (error) {
      console.error('Failed to select LUT file:', error);
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

    const file = files[0];
    const filePath = (file as any).path || file.name; // Tauri provides file.path
    
    if (type === 'video') {
      const videoExtensions = ['mp4', 'mov', 'avi', 'mkv', 'wmv', 'flv', 'webm', 'm4v'];
      const extension = file.name.split('.').pop()?.toLowerCase();
      
      if (extension && videoExtensions.includes(extension)) {
        const fileInfo: FileInfo = {
          name: file.name,
          path: filePath,
          size: file.size,
          type: file.type
        };
        setVideoFile(fileInfo);
        onVideoSelect(filePath);
      }
    } else if (type === 'lut') {
      const lutExtensions = ['cube', '3dl', 'lut', 'csp', 'vlt', 'mga', 'm3d', 'look'];
      const extension = file.name.split('.').pop()?.toLowerCase();
      
      if (extension && lutExtensions.includes(extension)) {
        const fileInfo: FileInfo = {
          name: file.name,
          path: filePath,
          size: file.size,
          type: file.type
        };
        setLutFile(fileInfo);
        onLutSelect(filePath);
      }
    }
  }, [disabled, loading, onVideoSelect, onLutSelect]);

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
  const clearVideoFile = () => {
    setVideoFile(null);
  };

  const clearLutFile = () => {
    setLutFile(null);
  };

  return (
    <div className="file-upload">
      <div className="upload-section">
        <h3>选择文件</h3>
        
        {/* 视频文件上传区域 */}
        <div 
          className={`upload-area ${
            dragOver === 'video' ? 'drag-over' : ''
          } ${disabled ? 'disabled' : ''}`}
          onDragOver={(e) => handleDragOver(e, 'video')}
          onDragLeave={handleDragLeave}
          onDrop={(e) => handleDrop(e, 'video')}
          onClick={selectVideoFile}
        >
          <div className="upload-content">
            {videoFile ? (
              <div className="file-info">
                <div className="file-icon video-icon">🎬</div>
                <div className="file-details">
                  <div className="file-name">{videoFile.name}</div>
                  <div className="file-meta">
                    {videoFile.size && (
                      <span className="file-size">{formatFileSize(videoFile.size)}</span>
                    )}
                  </div>
                </div>
                <button 
                  className="clear-button"
                  onClick={(e) => {
                    e.stopPropagation();
                    clearVideoFile();
                  }}
                  disabled={disabled}
                >
                  ✕
                </button>
              </div>
            ) : (
              <div className="upload-placeholder">
                <div className="upload-icon">📹</div>
                <div className="upload-text">
                  <div className="primary-text">选择视频文件</div>
                  <div className="secondary-text">
                    支持 MP4, MOV, AVI, MKV 等格式
                  </div>
                  <div className="hint-text">
                    点击选择或拖拽文件到此处
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>

        {/* LUT文件上传区域 */}
        <div 
          className={`upload-area ${
            dragOver === 'lut' ? 'drag-over' : ''
          } ${disabled ? 'disabled' : ''}`}
          onDragOver={(e) => handleDragOver(e, 'lut')}
          onDragLeave={handleDragLeave}
          onDrop={(e) => handleDrop(e, 'lut')}
          onClick={selectLutFile}
        >
          <div className="upload-content">
            {lutFile ? (
              <div className="file-info">
                <div className="file-icon lut-icon">🎨</div>
                <div className="file-details">
                  <div className="file-name">{lutFile.name}</div>
                  <div className="file-meta">
                    {lutFile.size && (
                      <span className="file-size">{formatFileSize(lutFile.size)}</span>
                    )}
                  </div>
                </div>
                <button 
                  className="clear-button"
                  onClick={(e) => {
                    e.stopPropagation();
                    clearLutFile();
                  }}
                  disabled={disabled}
                >
                  ✕
                </button>
              </div>
            ) : (
              <div className="upload-placeholder">
                <div className="upload-icon">🎨</div>
                <div className="upload-text">
                  <div className="primary-text">选择LUT文件</div>
                  <div className="secondary-text">
                    支持 CUBE, 3DL, LUT, CSP 等格式
                  </div>
                  <div className="hint-text">
                    点击选择或拖拽文件到此处
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>

      {loading && (
        <div className="loading-overlay">
          <div className="loading-spinner"></div>
          <div className="loading-text">正在处理文件...</div>
        </div>
      )}
    </div>
  );
};

export default FileUpload;