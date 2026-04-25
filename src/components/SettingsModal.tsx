import React, { useState, useEffect, useCallback } from 'react';
import { createPortal } from 'react-dom';
import { invoke } from '@tauri-apps/api/core';
import './SettingsModal.css';

interface SettingsModalProps {
  isOpen: boolean;
  settings: ProcessingSettings;
  onClose: () => void;
  onSettingsChange: (settings: ProcessingSettings) => void;
  disabled?: boolean;
}

interface ProcessingSettings {
  output_format: string;
  video_codec: string;
  audio_codec: string;
  quality_preset: string;
  resolution: string;
  fps: number | null;
  bitrate: string;
  lut_intensity: number;
  lut_error_strategy: 'StopOnError' | 'SkipOnError';
  color_space: string;
  hardware_acceleration: boolean;
  two_pass_encoding: boolean;
  preserve_metadata: boolean;
  output_directory: string;
}

interface CodecInfo {
  name: string;
  description: string;
  supported: boolean;
}

interface QualityPreset {
  name: string;
  description: string;
  crf: number;
  preset: string;
}

interface GpuInfo {
  name: string;
  vendor: string;
  memory_total?: number | null;
  memory_used?: number | null;
  temperature?: number | null;
  utilization?: number | null;
  supports_hardware_acceleration: boolean;
}

interface HardwareAccelerationInfo {
  available: boolean;
  supported_codecs: string[];
  recommended_settings: string[];
}

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

// 运行时检测是否处于 Tauri 环境
const isTauriEnv = () => typeof window !== 'undefined' && !!(window as any).__TAURI_INTERNALS__;

// 安全调用 Tauri invoke（在浏览器预览中优雅降级）
async function safeInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (typeof invoke !== 'function' || !isTauriEnv()) {
    throw new Error('tauri_unavailable');
  }

  try {
    return await invoke<T>(cmd, args);
  } catch (error) {
    if (error instanceof Error) {
      throw error;
    }
    throw new Error(cmd);
  }
}

const SettingsModal: React.FC<SettingsModalProps> = ({
  isOpen,
  settings: initialSettings,
  onClose,
  onSettingsChange,
  disabled = false
}) => {
  const DEFAULT_SETTINGS: ProcessingSettings = {
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
  };

  const [settings, setSettings] = useState<ProcessingSettings>(initialSettings);
  const [availableCodecs, setAvailableCodecs] = useState<{
    video: CodecInfo[];
    audio: CodecInfo[];
  }>({ video: [], audio: [] });
  const [loading, setLoading] = useState(false);
  const [gpuLoading, setGpuLoading] = useState(false);
  const [gpuError, setGpuError] = useState<string | null>(null);
  const [gpuInfos, setGpuInfos] = useState<GpuInfo[]>([]);
  const [hwAccelInfo, setHwAccelInfo] = useState<HardwareAccelerationInfo | null>(null);
  const [codecTestLoading, setCodecTestLoading] = useState(false);
  const [codecTestResult, setCodecTestResult] = useState<boolean | null>(null);
  const [systemInfo, setSystemInfo] = useState<SystemInfo | null>(null);
  const [systemInfoError, setSystemInfoError] = useState<string | null>(null);
  const [systemInfoLoading, setSystemInfoLoading] = useState(false);
  const [cacheSize, setCacheSize] = useState<number | null>(null);
  const [cacheError, setCacheError] = useState<string | null>(null);
  const [cacheLoading, setCacheLoading] = useState(false);
  const [cacheActionLoading, setCacheActionLoading] = useState(false);
  const [cacheActionMessage, setCacheActionMessage] = useState<string | null>(null);
  const [logFiles, setLogFiles] = useState<string[]>([]);
  const [logFilesLoading, setLogFilesLoading] = useState(false);
  const [logFilesError, setLogFilesError] = useState<string | null>(null);
  const [selectedLogFile, setSelectedLogFile] = useState<string | null>(null);
  const [logContent, setLogContent] = useState('');
  const [logContentLoading, setLogContentLoading] = useState(false);
  const [logContentError, setLogContentError] = useState<string | null>(null);
  const [hasLoadedLogs, setHasLoadedLogs] = useState(false);

  // 质量预设选项
  const qualityPresets: QualityPreset[] = [
    {
      name: 'high_quality',
      description: '高质量 (较大文件)',
      crf: 18,
      preset: 'slow'
    },
    {
      name: 'balanced',
      description: '平衡 (推荐)',
      crf: 23,
      preset: 'medium'
    },
    {
      name: 'fast',
      description: '快速 (较低质量)',
      crf: 28,
      preset: 'fast'
    },
    {
      name: 'web_optimized',
      description: '网络优化',
      crf: 25,
      preset: 'medium'
    }
  ];

  // 分辨率选项
  const resolutionOptions = [
    { value: 'original', label: '保持原分辨率' },
    { value: '3840x2160', label: '4K (3840×2160)' },
    { value: '2560x1440', label: '2K (2560×1440)' },
    { value: '1920x1080', label: '1080p (1920×1080)' },
    { value: '1280x720', label: '720p (1280×720)' },
    { value: '854x480', label: '480p (854×480)' }
  ];

  // 色彩空间选项
  const colorSpaceOptions = [
    { value: 'rec709', label: 'Rec.709 (标准)' },
    { value: 'rec2020', label: 'Rec.2020 (HDR)' },
    { value: 'srgb', label: 'sRGB' },
    { value: 'adobe_rgb', label: 'Adobe RGB' },
    { value: 'dci_p3', label: 'DCI-P3' }
  ];

  // 输出格式选项
  const formatOptions = [
    { value: 'mp4', label: 'MP4 (推荐)' },
    { value: 'mov', label: 'MOV (QuickTime)' },
    { value: 'avi', label: 'AVI' },
    { value: 'mkv', label: 'MKV (Matroska)' },
    { value: 'webm', label: 'WebM' }
  ];

  // 加载可用编解码器
  const loadAvailableCodecs = useCallback(async () => {
    try {
      setLoading(true);
      const codecs = await safeInvoke<{ video_codecs: CodecInfo[]; audio_codecs: CodecInfo[] }>('get_available_codecs');
      setAvailableCodecs({ video: codecs.video_codecs, audio: codecs.audio_codecs });
    } catch {
      // 浏览器预览或失败：使用默认编解码器列表（静默降级）
      setAvailableCodecs({
        video: [
          { name: 'libx264', description: 'H.264 (推荐)', supported: true },
          { name: 'libx265', description: 'H.265/HEVC', supported: true },
          { name: 'libvpx-vp9', description: 'VP9', supported: true },
          { name: 'libaom-av1', description: 'AV1', supported: false }
        ],
        audio: [
          { name: 'aac', description: 'AAC (推荐)', supported: true },
          { name: 'mp3', description: 'MP3', supported: true },
          { name: 'opus', description: 'Opus', supported: true },
          { name: 'flac', description: 'FLAC (无损)', supported: true }
        ]
      });
    } finally {
      setLoading(false);
    }
  }, []);

  const formatMemory = useCallback((bytes?: number | null): string => {
    if (!bytes || bytes <= 0) return '未知';
    const gb = bytes / (1024 * 1024 * 1024);
    if (gb >= 1) return `${gb.toFixed(1)} GB`;
    const mb = bytes / (1024 * 1024);
    return `${mb.toFixed(0)} MB`;
  }, []);

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

  const loadGpuDiagnostics = useCallback(async () => {
    try {
      setGpuLoading(true);
      setGpuError(null);

      const [gpus, hw] = await Promise.all([
        safeInvoke<GpuInfo[]>('get_gpu_info'),
        safeInvoke<HardwareAccelerationInfo>('check_hardware_acceleration')
      ]);

      setGpuInfos(Array.isArray(gpus) ? gpus : []);
      setHwAccelInfo(hw);
    } catch (error) {
      const message = error instanceof Error ? error.message : 'unknown_error';
      if (message === 'tauri_unavailable') {
        setGpuError('当前环境不支持实时 GPU 诊断（浏览器预览模式）');
      } else {
        setGpuError('GPU 诊断失败，请检查 FFmpeg 和显卡驱动');
      }
      setGpuInfos([]);
      setHwAccelInfo(null);
    } finally {
      setGpuLoading(false);
    }
  }, []);

  const loadSystemDiagnostics = useCallback(async () => {
    try {
      setSystemInfoLoading(true);
      setCacheLoading(true);
      setSystemInfoError(null);
      setCacheError(null);
      setCacheActionMessage(null);

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
      setSystemInfo(null);
      setCacheSize(null);
    } finally {
      setSystemInfoLoading(false);
      setCacheLoading(false);
    }
  }, []);

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

  const loadLogFiles = useCallback(async () => {
    try {
      setLogFilesLoading(true);
      setLogFilesError(null);
      const files = await safeInvoke<string[]>('get_log_files');
      setLogFiles(files);
      setHasLoadedLogs(true);
      if (!files.includes(selectedLogFile || '')) {
        setSelectedLogFile(null);
        setLogContent('');
      }
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
  }, [selectedLogFile]);

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
      setLogContent('');
    } finally {
      setLogContentLoading(false);
    }
  }, []);

  const testCurrentCodec = useCallback(async () => {
    try {
      setCodecTestLoading(true);
      setCodecTestResult(null);
      setGpuError(null);
      const ok = await safeInvoke<boolean>('test_hardware_acceleration', {
        codec: settings.video_codec
      });
      setCodecTestResult(!!ok);
    } catch (error) {
      const message = error instanceof Error ? error.message : 'unknown_error';
      if (message === 'tauri_unavailable') {
        setGpuError('当前环境不支持编码器测试（浏览器预览模式）');
      } else {
        setGpuError(`编码器测试失败：${settings.video_codec}`);
      }
      setCodecTestResult(false);
    } finally {
      setCodecTestLoading(false);
    }
  }, [settings.video_codec]);

  // 更新设置
  const updateSetting = useCallback((key: keyof ProcessingSettings, value: any) => {
    const newSettings = { ...settings, [key]: value };
    setSettings(newSettings);
    onSettingsChange(newSettings);
  }, [settings, onSettingsChange]);

  // 重置为默认设置
  const resetToDefaults = useCallback(() => {
    setSettings(DEFAULT_SETTINGS);
    onSettingsChange(DEFAULT_SETTINGS);
  }, [onSettingsChange]);

  useEffect(() => {
    setSettings(initialSettings);
  }, [initialSettings]);

  // 组件挂载时加载编解码器
  useEffect(() => {
    if (isOpen) {
      loadAvailableCodecs();
      loadGpuDiagnostics();
      loadSystemDiagnostics();
      setCodecTestResult(null);
    }
  }, [isOpen, loadAvailableCodecs, loadGpuDiagnostics, loadSystemDiagnostics]);

  // ESC键关闭弹窗
  useEffect(() => {
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && isOpen) {
        onClose();
      }
    };

    document.addEventListener('keydown', handleEscape);
    return () => document.removeEventListener('keydown', handleEscape);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  const modalContent = (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-content settings-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>处理设置</h2>
          <button className="btn-close" onClick={onClose} aria-label="关闭">
            <svg width="24" height="24" viewBox="0 0 24 24" fill="none">
              <path d="M18 6L6 18M6 6l12 12" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
            </svg>
          </button>
        </div>

        <div className="modal-body">
          {loading && (
            <div className="loading-state">
              <div className="loading-spinner"></div>
              <span>正在加载设置...</span>
            </div>
          )}

          <div className="settings-grid">
            {/* 基础设置 */}
            <div className="settings-section">
              <h3>基础设置</h3>
              
              <div className="setting-group">
                <label className="setting-label">输出格式</label>
                <select
                  className="setting-select"
                  value={settings.output_format}
                  onChange={(e) => updateSetting('output_format', e.target.value)}
                  disabled={disabled}
                >
                  {formatOptions.map(option => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </select>
              </div>

              <div className="setting-group">
                <label className="setting-label">质量预设</label>
                <select
                  className="setting-select"
                  value={settings.quality_preset}
                  onChange={(e) => updateSetting('quality_preset', e.target.value)}
                  disabled={disabled}
                >
                  {qualityPresets.map(preset => (
                    <option key={preset.name} value={preset.name}>
                      {preset.description}
                    </option>
                  ))}
                </select>
              </div>

              <div className="setting-group">
                <label className="setting-label">输出分辨率</label>
                <select
                  className="setting-select"
                  value={settings.resolution}
                  onChange={(e) => updateSetting('resolution', e.target.value)}
                  disabled={disabled}
                >
                  {resolutionOptions.map(option => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </select>
              </div>
            </div>

            {/* 编码设置 */}
            <div className="settings-section">
              <h3>编码设置</h3>
              
              <div className="setting-group">
                <label className="setting-label">视频编码器</label>
                <select
                  className="setting-select"
                  value={settings.video_codec}
                  onChange={(e) => updateSetting('video_codec', e.target.value)}
                  disabled={disabled}
                >
                  {availableCodecs.video.map(codec => (
                    <option 
                      key={codec.name} 
                      value={codec.name}
                      disabled={!codec.supported}
                    >
                      {codec.description}
                    </option>
                  ))}
                </select>
              </div>

              <div className="setting-group">
                <label className="setting-label">音频编码器</label>
                <select
                  className="setting-select"
                  value={settings.audio_codec}
                  onChange={(e) => updateSetting('audio_codec', e.target.value)}
                  disabled={disabled}
                >
                  {availableCodecs.audio.map(codec => (
                    <option 
                      key={codec.name} 
                      value={codec.name}
                      disabled={!codec.supported}
                    >
                      {codec.description}
                    </option>
                  ))}
                </select>
              </div>

              <div className="setting-group">
                <label className="setting-label">色彩空间</label>
                <select
                  className="setting-select"
                  value={settings.color_space}
                  onChange={(e) => updateSetting('color_space', e.target.value)}
                  disabled={disabled}
                >
                  {colorSpaceOptions.map(option => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </select>
              </div>
            </div>

            {/* LUT设置 */}
            <div className="settings-section">
              <h3>LUT设置</h3>
              
              <div className="setting-group">
                <label className="setting-label">LUT强度</label>
                <div className="slider-container">
                  <input
                    type="range"
                    className="setting-slider"
                    min="0"
                    max="100"
                    value={settings.lut_intensity}
                    onChange={(e) => updateSetting('lut_intensity', parseInt(e.target.value))}
                    disabled={disabled}
                  />
                  <span className="slider-value">{settings.lut_intensity}%</span>
                </div>
              </div>

              <div className="setting-group">
                <label className="setting-label">无效LUT处理</label>
                <select
                  className="setting-select"
                  value={settings.lut_error_strategy}
                  onChange={(e) => updateSetting('lut_error_strategy', e.target.value)}
                  disabled={disabled}
                >
                  <option value="StopOnError">停止并提示错误</option>
                  <option value="SkipOnError">跳过无效LUT继续处理</option>
                </select>
              </div>
            </div>

            {/* 高级设置 */}
            <div className="settings-section">
              <h3>高级设置</h3>
              
              <div className="setting-group checkbox-group">
                <label className="checkbox-label">
                  <input
                    type="checkbox"
                    checked={settings.hardware_acceleration}
                    onChange={(e) => updateSetting('hardware_acceleration', e.target.checked)}
                    disabled={disabled}
                  />
                  <span className="checkbox-text">硬件加速</span>
                </label>
              </div>

              <div className="setting-group checkbox-group">
                <label className="checkbox-label">
                  <input
                    type="checkbox"
                    checked={settings.two_pass_encoding}
                    onChange={(e) => updateSetting('two_pass_encoding', e.target.checked)}
                    disabled={disabled}
                  />
                  <span className="checkbox-text">双通道编码</span>
                </label>
              </div>

              <div className="setting-group checkbox-group">
                <label className="checkbox-label">
                  <input
                    type="checkbox"
                    checked={settings.preserve_metadata}
                    onChange={(e) => updateSetting('preserve_metadata', e.target.checked)}
                    disabled={disabled}
                  />
                  <span className="checkbox-text">保留元数据</span>
                </label>
              </div>

              <div className="setting-group gpu-diagnostics">
                <div className="gpu-header">
                  <span className="setting-label">GPU 诊断</span>
                  <button
                    className="btn-secondary btn-small"
                    onClick={loadGpuDiagnostics}
                    disabled={disabled || gpuLoading || codecTestLoading}
                    type="button"
                  >
                    {gpuLoading ? '检测中...' : '刷新'}
                  </button>
                </div>

                {gpuError && <div className="gpu-error">{gpuError}</div>}

                {!gpuLoading && gpuInfos.length > 0 && (
                  <div className="gpu-list">
                    {gpuInfos.map((gpu, index) => (
                      <div className="gpu-item" key={`${gpu.name}-${index}`}>
                        <div className="gpu-title">{gpu.name}</div>
                        <div className="gpu-meta">
                          厂商: {gpu.vendor || '未知'} · 显存: {formatMemory(gpu.memory_total)}
                        </div>
                        <div className="gpu-meta">
                          加速支持: {gpu.supports_hardware_acceleration ? '支持' : '不支持'}
                          {typeof gpu.utilization === 'number' ? ` · 占用: ${gpu.utilization.toFixed(0)}%` : ''}
                          {typeof gpu.temperature === 'number' ? ` · 温度: ${gpu.temperature.toFixed(0)}°C` : ''}
                        </div>
                      </div>
                    ))}
                  </div>
                )}

                {hwAccelInfo && (
                  <div className="gpu-summary">
                    <div className="gpu-meta">
                      硬件加速: {hwAccelInfo.available ? '可用' : '不可用'}
                    </div>
                    {hwAccelInfo.supported_codecs.length > 0 && (
                      <div className="gpu-meta">
                        支持能力: {hwAccelInfo.supported_codecs.join(', ')}
                      </div>
                    )}
                    {hwAccelInfo.recommended_settings.length > 0 && (
                      <div className="gpu-meta">
                        推荐参数: {hwAccelInfo.recommended_settings.slice(0, 3).join(' | ')}
                      </div>
                    )}
                  </div>
                )}

                <div className="gpu-test-row">
                  <button
                    className="btn-secondary btn-small"
                    onClick={testCurrentCodec}
                    disabled={disabled || codecTestLoading || gpuLoading}
                    type="button"
                  >
                    {codecTestLoading ? '测试中...' : `测试编码器: ${settings.video_codec}`}
                  </button>
                  {codecTestResult !== null && (
                    <span className={`gpu-test-result ${codecTestResult ? 'success' : 'failed'}`}>
                      {codecTestResult ? '可用于硬件加速' : '不建议用于硬件加速'}
                    </span>
                  )}
                </div>
              </div>
            </div>

            <div className="settings-section">
              <h3>系统与缓存</h3>

              <div className="setting-group gpu-diagnostics">
                <div className="gpu-header">
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
                  <div className="gpu-list">
                    <div className="gpu-item">
                      <div className="gpu-title">{systemInfo.system_name} {systemInfo.system_version}</div>
                      <div className="gpu-meta">CPU 核心: {systemInfo.cpu_count}</div>
                      <div className="gpu-meta">CPU 占用: {systemInfo.cpu_usage.toFixed(1)}%</div>
                      <div className="gpu-meta">内存占用: {systemInfo.memory_usage.toFixed(1)}%</div>
                    </div>
                  </div>
                )}

                <div className="gpu-summary">
                  <div className="gpu-meta">
                    缓存大小: {cacheLoading ? '读取中...' : formatBytes(cacheSize)}
                  </div>
                  <div className="gpu-test-row">
                    <button
                      className="btn-secondary btn-small"
                      onClick={handleClearCache}
                      disabled={disabled || cacheActionLoading || cacheLoading}
                      type="button"
                    >
                      {cacheActionLoading ? '清理中...' : '清理缓存'}
                    </button>
                    {cacheActionMessage && (
                      <span className="gpu-test-result success">{cacheActionMessage}</span>
                    )}
                  </div>
                  {cacheError && <div className="gpu-error">{cacheError}</div>}
                </div>
              </div>
            </div>

            <div className="settings-section">
              <h3>日志</h3>

              <div className="setting-group gpu-diagnostics diagnostics-card">
                <div className="gpu-header">
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
                  <div className="gpu-meta">暂无日志文件</div>
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
          </div>
        </div>

        <div className="modal-footer">
          <button className="btn-secondary" onClick={resetToDefaults} disabled={disabled}>
            重置默认
          </button>
          <button className="btn-primary" onClick={onClose}>
            确定
          </button>
        </div>
      </div>
    </div>
  );

  return createPortal(modalContent, document.body);
};

export default SettingsModal;
