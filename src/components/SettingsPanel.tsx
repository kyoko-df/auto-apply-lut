import React, { useState, useEffect, useCallback } from 'react';
// 移除直接导入 invoke，避免在纯浏览器预览时报错
// import { invoke } from '@tauri-apps/api/core';
import './SettingsPanel.css';

interface SettingsPanelProps {
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

// 运行时检测是否处于 Tauri 环境
const isTauriEnv = () => typeof window !== 'undefined' && !!(window as any).__TAURI_INTERNALS__;

// 安全调用 Tauri invoke（在浏览器预览中优雅降级）
async function safeInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    const mod = await import('@tauri-apps/api/core');
    if (typeof mod?.invoke === 'function' && isTauriEnv()) {
      return await mod.invoke<T>(cmd, args);
    }
    throw new Error('tauri_unavailable');
  } catch (e) {
    throw new Error('tauri_unavailable');
  }
}

// FFmpeg 信息接口（与后端结构对应）
interface FfmpegInfo {
  mode: 'native' | 'external';
  static_linked: boolean;
  library_versions?: Record<string, string> | null;
  binary_version?: string | null;
  binary_path?: string | null;
}

const SettingsPanel: React.FC<SettingsPanelProps> = ({
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
    color_space: 'rec709',
    hardware_acceleration: false,
    two_pass_encoding: false,
    preserve_metadata: true,
    output_directory: ''
  };
  const [settings, setSettings] = useState<ProcessingSettings>(DEFAULT_SETTINGS);
  const [availableCodecs, setAvailableCodecs] = useState<{
    video: CodecInfo[];
    audio: CodecInfo[];
  }>({ video: [], audio: [] });

  const [isExpanded, setIsExpanded] = useState(false);
  const [loading, setLoading] = useState(false);

  // FFmpeg 路径与状态
  const [ffmpegPath, setFfmpegPath] = useState<string>('');
  const [ffmpegStatus, setFfmpegStatus] = useState<
    { state: 'unknown' | 'ok' | 'missing' | 'error'; message?: string; info?: FfmpegInfo }
  >({ state: 'unknown' });
  const [savingFfmpeg, setSavingFfmpeg] = useState(false);

  const ffmpegInfoMessage = useCallback((info: FfmpegInfo) => {
    if (info.mode === 'native') return '已链接本地库';
    const version = info.binary_version?.trim() ? info.binary_version.trim() : 'FFmpeg';
    const location = info.binary_path?.trim() ? ` | ${info.binary_path.trim()}` : '';
    return `${version}${location}`;
  }, []);

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
          { name: 'libvpx-vp9', description: 'VP9', supported: true }
        ],
        audio: [
          { name: 'aac', description: 'AAC (推荐)', supported: true },
          { name: 'mp3', description: 'MP3', supported: true },
          { name: 'opus', description: 'Opus', supported: true }
        ]
      });
    } finally {
      setLoading(false);
    }
  }, []);

  // 初始化：加载编解码器、FFmpeg 路径与状态
  useEffect(() => {
    loadAvailableCodecs();

    (async () => {
      // 读取已保存的路径
      try {
        const cfg = await safeInvoke<{ ffmpeg_path: string | null }>('get_ffmpeg_path_config');
        setFfmpegPath(cfg?.ffmpeg_path ?? '');
      } catch {
        // ignore in web preview
      }

      // 检测 FFmpeg 可用性
      try {
        const info = await safeInvoke<FfmpegInfo>('get_ffmpeg_info');
        setFfmpegStatus({ state: 'ok', info, message: ffmpegInfoMessage(info) });
      } catch (e) {
        setFfmpegStatus({ state: 'missing', message: '未检测到可用的 FFmpeg，可设置自定义路径' });
        setIsExpanded(true); // 引导用户展开设置
      }
    })();
  }, [loadAvailableCodecs, ffmpegInfoMessage]);

  // 设置变化时通知父组件
  useEffect(() => {
    onSettingsChange(settings);
  }, [settings, onSettingsChange]);

  // 更新设置
  const updateSetting = useCallback(<K extends keyof ProcessingSettings>(
    key: K,
    value: ProcessingSettings[K]
  ) => {
    setSettings(prev => ({ ...prev, [key]: value }));
  }, []);

  // 选择输出目录（使用 tauri dialog 插件）
  const selectOutputDirectory = useCallback(async () => {
    try {
      if (!isTauriEnv()) throw new Error('tauri_unavailable');
      const dlg = await import('@tauri-apps/plugin-dialog');
      const directory = await (dlg as any).open({ directory: true, multiple: false });
      if (typeof directory === 'string' && directory) {
        updateSetting('output_directory', directory);
      }
    } catch (error) {
      // 非 Tauri 环境下不弹错，保持输入框可手动填写
      // console.debug('Directory selection not available in web preview');
    }
  }, [updateSetting]);

  // 浏览选择 FFmpeg 可执行文件
  const browseFfmpegPath = useCallback(async () => {
    try {
      if (!isTauriEnv()) throw new Error('tauri_unavailable');
      const dlg = await import('@tauri-apps/plugin-dialog');
      const selected = await (dlg as any).open({
        multiple: false,
        directory: false,
        filters: [
          { name: 'FFmpeg Executable', extensions: ['exe', ''] }
        ]
      });
      if (typeof selected === 'string' && selected) {
        setFfmpegPath(selected);
      }
    } catch (e) {
      // ignore in web preview
    }
  }, []);

  // 保存 FFmpeg 路径到后端配置
  const saveFfmpegPath = useCallback(async () => {
    try {
      setSavingFfmpeg(true);
      await safeInvoke<void>('set_ffmpeg_path_config', { path: ffmpegPath.trim() ? ffmpegPath.trim() : null });
      // 保存后立即重新检测
      try {
        const info = await safeInvoke<FfmpegInfo>('get_ffmpeg_info');
        setFfmpegStatus({ state: 'ok', info, message: ffmpegInfoMessage(info) });
      } catch (e) {
        setFfmpegStatus({ state: 'error', message: '保存成功，但仍无法检测到 FFmpeg，请确认路径是否正确' });
      }
    } catch (e) {
      setFfmpegStatus({ state: 'error', message: '保存失败，请重试或检查权限' });
    } finally {
      setSavingFfmpeg(false);
    }
  }, [ffmpegPath]);

  // 手动检测
  const testDetectFfmpeg = useCallback(async () => {
    try {
      const info = await safeInvoke<FfmpegInfo>('get_ffmpeg_info');
      setFfmpegStatus({ state: 'ok', info, message: ffmpegInfoMessage(info) });
    } catch (e) {
      setFfmpegStatus({ state: 'missing', message: '未检测到可用的 FFmpeg，请设置自定义路径' });
    }
  }, [ffmpegInfoMessage]);

  const resetToDefaults = useCallback(() => {
    setSettings({ ...DEFAULT_SETTINGS });
  }, []);

  return (
    <div className={`settings-panel ${isExpanded ? 'expanded' : ''}`}>
      <div className="settings-header" onClick={() => setIsExpanded(!isExpanded)}>
        <h3>处理设置</h3>
        <button className="expand-button">
          {isExpanded ? '▼' : '▶'}
        </button>
      </div>

      {isExpanded && (
        <div className="settings-content">
          {loading && (
            <div className="loading-state">
              <div className="loading-spinner"></div>
              <span>正在加载设置...</span>
            </div>
          )}

          {/* FFmpeg 设置区 */}
          <div className="ffmpeg-section">
            <div className="ffmpeg-header">
              <h4>FFmpeg 设置</h4>
              {ffmpegStatus.state === 'ok' && (
                <span className="status-pill ok">已检测到</span>
              )}
              {ffmpegStatus.state === 'missing' && (
                <span className="status-pill warn">未检测到</span>
              )}
              {ffmpegStatus.state === 'error' && (
                <span className="status-pill error">异常</span>
              )}
            </div>
            {ffmpegStatus.message && (
              <div className="ffmpeg-message">{ffmpegStatus.message}</div>
            )}

            <div className="directory-input" style={{ marginTop: 8 }}>
              <input
                type="text"
                className="setting-input"
                placeholder="例如 C:\\ffmpeg\\bin\\ffmpeg.exe，留空则使用系统 PATH 查找"
                value={ffmpegPath}
                onChange={(e) => setFfmpegPath(e.target.value)}
                disabled={disabled}
              />
              <button
                className="browse-button"
                onClick={browseFfmpegPath}
                disabled={disabled || !isTauriEnv()}
                title={isTauriEnv() ? '选择 ffmpeg 可执行文件' : '浏览器预览中不可用'}
              >
                浏览
              </button>
              <button
                className="btn-primary"
                onClick={saveFfmpegPath}
                disabled={disabled || savingFfmpeg}
                title="保存路径并重新检测"
              >
                {savingFfmpeg ? '保存中...' : '保存'}
              </button>
            </div>
            <div className="settings-actions" style={{ justifyContent: 'flex-start' }}>
              <button className="ghost" onClick={testDetectFfmpeg} disabled={disabled}>重新检测</button>
              <button className="ghost" onClick={() => setFfmpegPath('')} disabled={disabled}>清除路径</button>
            </div>
          </div>

          <div className="settings-grid">
            {/* 输出格式 */}
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

            {/* 质量预设 */}
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

            {/* 视频编解码器 */}
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

            {/* 音频编解码器 */}
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

            {/* 分辨率 */}
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

            {/* 帧率 */}
            <div className="setting-group">
              <label className="setting-label">帧率 (fps)</label>
              <input
                type="number"
                className="setting-input"
                value={settings.fps ?? ''}
                onChange={(e) => updateSetting('fps', e.target.value === '' ? null : Number(e.target.value))}
                placeholder="保持原帧率留空"
                disabled={disabled}
                min={1}
                max={240}
              />
            </div>

            {/* 比特率 */}
            <div className="setting-group">
              <label className="setting-label">比特率</label>
              <input
                type="text"
                className="setting-input"
                value={settings.bitrate}
                onChange={(e) => updateSetting('bitrate', e.target.value)}
                placeholder="auto 或 4000k / 5M 等"
                disabled={disabled}
              />
            </div>

            {/* LUT 强度 */}
            <div className="setting-group">
              <label className="setting-label">LUT 强度 ({settings.lut_intensity}%)</label>
              <input
                type="range"
                className="setting-slider"
                value={settings.lut_intensity}
                onChange={(e) => updateSetting('lut_intensity', Number(e.target.value))}
                min={0}
                max={100}
                disabled={disabled}
              />
            </div>

            {/* 色彩空间 */}
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

          {/* 高级选项 */}
          <div className="advanced-options">
            <h4>高级选项</h4>
            
            <div className="checkbox-group">
              <label className="checkbox-label">
                <input
                  type="checkbox"
                  checked={settings.hardware_acceleration}
                  onChange={(e) => updateSetting('hardware_acceleration', e.target.checked)}
                  disabled={disabled}
                />
                <span className="checkbox-text">硬件加速</span>
                <span className="checkbox-description">使用GPU加速编码 (如果支持)</span>
              </label>

              <label className="checkbox-label">
                <input
                  type="checkbox"
                  checked={settings.two_pass_encoding}
                  onChange={(e) => updateSetting('two_pass_encoding', e.target.checked)}
                  disabled={disabled}
                />
                <span className="checkbox-text">双通道编码</span>
                <span className="checkbox-description">更好的质量，但处理时间更长</span>
              </label>

              <label className="checkbox-label">
                <input
                  type="checkbox"
                  checked={settings.preserve_metadata}
                  onChange={(e) => updateSetting('preserve_metadata', e.target.checked)}
                  disabled={disabled}
                />
                <span className="checkbox-text">保留元数据</span>
                <span className="checkbox-description">保留原始文件的元数据信息</span>
              </label>
            </div>
          </div>

          {/* 输出目录 */}
          <div className="output-directory">
            <label className="setting-label">输出目录</label>
            <div className="directory-input">
              <input
                type="text"
                className="setting-input"
                value={settings.output_directory}
                onChange={(e) => updateSetting('output_directory', e.target.value)}
                placeholder="默认为视频文件所在目录"
                disabled={disabled}
              />
              <button
                className="browse-button"
                onClick={selectOutputDirectory}
                disabled={disabled || !isTauriEnv()}
                title={isTauriEnv() ? '选择输出目录' : '浏览器预览中不可用'}
              >
                浏览
              </button>
            </div>
          </div>

          {/* 操作按钮 */}
          <div className="settings-actions">
            <button
              className="reset-button"
              onClick={resetToDefaults}
              disabled={disabled}
            >
              重置默认
            </button>
          </div>
        </div>
      )}
    </div>
  );
};

export default SettingsPanel;
