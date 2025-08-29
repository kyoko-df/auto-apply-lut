import React, { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
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

const SettingsPanel: React.FC<SettingsPanelProps> = ({
  onSettingsChange,
  disabled = false
}) => {
  const [settings, setSettings] = useState<ProcessingSettings>({
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
  });

  const [availableCodecs, setAvailableCodecs] = useState<{
    video: CodecInfo[];
    audio: CodecInfo[];
  }>({ video: [], audio: [] });

  const [isExpanded, setIsExpanded] = useState(false);
  const [loading, setLoading] = useState(false);

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
      const codecs = await invoke<{
        video_codecs: CodecInfo[];
        audio_codecs: CodecInfo[];
      }>('get_available_codecs');
      
      setAvailableCodecs({
        video: codecs.video_codecs,
        audio: codecs.audio_codecs
      });
    } catch (error) {
      console.error('Failed to load codecs:', error);
      // 使用默认编解码器列表
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

  // 组件挂载时加载编解码器
  useEffect(() => {
    loadAvailableCodecs();
  }, [loadAvailableCodecs]);

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

  // 选择输出目录
  const selectOutputDirectory = useCallback(async () => {
    try {
      const directory = await invoke<string>('select_output_directory');
      if (directory) {
        updateSetting('output_directory', directory);
      }
    } catch (error) {
      console.error('Failed to select directory:', error);
    }
  }, [updateSetting]);

  // 重置为默认设置
  const resetToDefaults = useCallback(() => {
    setSettings({
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
    });
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
                value={settings.fps || ''}
                onChange={(e) => updateSetting('fps', e.target.value ? parseFloat(e.target.value) : null)}
                placeholder="保持原帧率"
                min="1"
                max="120"
                step="0.1"
                disabled={disabled}
              />
            </div>

            {/* 码率 */}
            <div className="setting-group">
              <label className="setting-label">码率</label>
              <input
                type="text"
                className="setting-input"
                value={settings.bitrate}
                onChange={(e) => updateSetting('bitrate', e.target.value)}
                placeholder="auto, 2M, 5000k"
                disabled={disabled}
              />
            </div>

            {/* LUT强度 */}
            <div className="setting-group">
              <label className="setting-label">
                LUT强度: {settings.lut_intensity}%
              </label>
              <input
                type="range"
                className="setting-slider"
                value={settings.lut_intensity}
                onChange={(e) => updateSetting('lut_intensity', parseInt(e.target.value))}
                min="0"
                max="100"
                step="1"
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
                disabled={disabled}
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