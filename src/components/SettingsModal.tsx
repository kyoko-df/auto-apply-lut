import React, { useState, useEffect, useCallback } from 'react';
import { createPortal } from 'react-dom';
import { Sliders, Video, Cpu, RotateCcw } from 'lucide-react';

interface SettingsModalProps {
  isOpen: boolean;
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

const isTauriEnv = () => typeof window !== 'undefined' && !!(window as any).__TAURI_INTERNALS__;

async function safeInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    const mod = await import('@tauri-apps/api/core');
    if (typeof mod?.invoke === 'function' && isTauriEnv()) {
      return await mod.invoke<T>(cmd, args);
    }
    throw new Error('tauri_unavailable');
  } catch {
    throw new Error('tauri_unavailable');
  }
}

const SettingsModal: React.FC<SettingsModalProps> = ({
  isOpen,
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
    color_space: 'rec709',
    hardware_acceleration: false,
    two_pass_encoding: false,
    preserve_metadata: true,
    output_directory: ''
  };

  const [settings, setSettings] = useState<ProcessingSettings>(DEFAULT_SETTINGS);
  const [activeTab, setActiveTab] = useState<'general' | 'video' | 'advanced'>('general');
  const [availableCodecs, setAvailableCodecs] = useState<{
    video: CodecInfo[];
    audio: CodecInfo[];
  }>({ video: [], audio: [] });
  const [loading, setLoading] = useState(false);

  const qualityPresets: QualityPreset[] = [
    { name: 'high_quality', description: '高质量 (较大文件)', crf: 18, preset: 'slow' },
    { name: 'balanced', description: '平衡 (推荐)', crf: 23, preset: 'medium' },
    { name: 'fast', description: '快速 (较低质量)', crf: 28, preset: 'fast' },
    { name: 'web_optimized', description: '网络优化', crf: 25, preset: 'medium' }
  ];

  const resolutionOptions = [
    { value: 'original', label: '保持原分辨率' },
    { value: '3840x2160', label: '4K (3840×2160)' },
    { value: '2560x1440', label: '2K (2560×1440)' },
    { value: '1920x1080', label: '1080p (1920×1080)' },
    { value: '1280x720', label: '720p (1280×720)' },
    { value: '854x480', label: '480p (854×480)' }
  ];

  const colorSpaceOptions = [
    { value: 'rec709', label: 'Rec.709 (标准)' },
    { value: 'rec2020', label: 'Rec.2020 (HDR)' },
    { value: 'srgb', label: 'sRGB' },
    { value: 'adobe_rgb', label: 'Adobe RGB' },
    { value: 'dci_p3', label: 'DCI-P3' }
  ];

  const formatOptions = [
    { value: 'mp4', label: 'MP4 (推荐)' },
    { value: 'mov', label: 'MOV (QuickTime)' },
    { value: 'avi', label: 'AVI' },
    { value: 'mkv', label: 'MKV (Matroska)' },
    { value: 'webm', label: 'WebM' }
  ];

  const loadAvailableCodecs = useCallback(async () => {
    try {
      setLoading(true);
      const codecs = await safeInvoke<{ video_codecs: CodecInfo[]; audio_codecs: CodecInfo[] }>('get_available_codecs');
      setAvailableCodecs({ video: codecs.video_codecs, audio: codecs.audio_codecs });
    } catch {
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

  const updateSetting = useCallback((key: keyof ProcessingSettings, value: any) => {
    const newSettings = { ...settings, [key]: value };
    setSettings(newSettings);
    onSettingsChange(newSettings);
  }, [settings, onSettingsChange]);

  const resetToDefaults = useCallback(() => {
    setSettings(DEFAULT_SETTINGS);
    onSettingsChange(DEFAULT_SETTINGS);
  }, [onSettingsChange]);

  useEffect(() => {
    if (isOpen) {
      loadAvailableCodecs();
    }
  }, [isOpen, loadAvailableCodecs]);

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

  return createPortal(
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 backdrop-blur-sm">
      <div className="w-[600px] bg-[var(--color-surface)] rounded-xl shadow-2xl border border-[var(--color-border)] overflow-hidden flex flex-col max-h-[85vh]">
        {/* Header */}
        <div className="h-12 flex items-center justify-between px-4 border-b border-[var(--color-border)] bg-[var(--color-surface-translucent)] backdrop-blur-md draggable">
          <div className="flex items-center gap-2">
            <div className="flex gap-1.5">
              <button onClick={onClose} className="w-3 h-3 rounded-full bg-[#FF5F57] hover:bg-[#FF3B30] border border-[#E0443E] transition-colors" />
              <div className="w-3 h-3 rounded-full bg-[#FEBC2E] border border-[#D89E24]" />
              <div className="w-3 h-3 rounded-full bg-[#28C840] border border-[#1AAB29]" />
            </div>
            <span className="ml-4 text-sm font-medium text-[var(--color-text-primary)]">偏好设置</span>
          </div>
        </div>

        {/* Toolbar */}
        <div className="flex justify-center p-2 bg-[var(--color-background)] border-b border-[var(--color-border)]">
          <div className="flex p-1 bg-[var(--color-border)] rounded-lg">
            <button
              onClick={() => setActiveTab('general')}
              className={`px-4 py-1.5 text-xs font-medium rounded-md transition-all ${activeTab === 'general' ? 'bg-[var(--color-surface)] shadow-sm text-[var(--color-text-primary)]' : 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)]'}`}
            >
              <div className="flex items-center gap-1.5">
                <Sliders size={14} />
                通用
              </div>
            </button>
            <button
              onClick={() => setActiveTab('video')}
              className={`px-4 py-1.5 text-xs font-medium rounded-md transition-all ${activeTab === 'video' ? 'bg-[var(--color-surface)] shadow-sm text-[var(--color-text-primary)]' : 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)]'}`}
            >
              <div className="flex items-center gap-1.5">
                <Video size={14} />
                视频
              </div>
            </button>
            <button
              onClick={() => setActiveTab('advanced')}
              className={`px-4 py-1.5 text-xs font-medium rounded-md transition-all ${activeTab === 'advanced' ? 'bg-[var(--color-surface)] shadow-sm text-[var(--color-text-primary)]' : 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)]'}`}
            >
              <div className="flex items-center gap-1.5">
                <Cpu size={14} />
                高级
              </div>
            </button>
          </div>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-6 bg-[var(--color-surface)]">
          {loading ? (
            <div className="flex items-center justify-center h-40 text-[var(--color-text-tertiary)]">
              <div className="animate-spin mr-2">
                <RotateCcw size={16} />
              </div>
              加载中...
            </div>
          ) : (
            <div className="space-y-6">
              {activeTab === 'general' && (
                <div className="space-y-4">
                  <div className="space-y-2">
                    <label className="text-xs font-medium text-[var(--color-text-secondary)] uppercase tracking-wider">输出格式</label>
                    <select
                      className="apple-input"
                      value={settings.output_format}
                      onChange={(e) => updateSetting('output_format', e.target.value)}
                      disabled={disabled}
                    >
                      {formatOptions.map(option => (
                        <option key={option.value} value={option.value}>{option.label}</option>
                      ))}
                    </select>
                  </div>

                  <div className="space-y-2">
                    <label className="text-xs font-medium text-[var(--color-text-secondary)] uppercase tracking-wider">质量预设</label>
                    <select
                      className="apple-input"
                      value={settings.quality_preset}
                      onChange={(e) => updateSetting('quality_preset', e.target.value)}
                      disabled={disabled}
                    >
                      {qualityPresets.map(preset => (
                        <option key={preset.name} value={preset.name}>{preset.description}</option>
                      ))}
                    </select>
                  </div>

                  <div className="space-y-2">
                    <label className="text-xs font-medium text-[var(--color-text-secondary)] uppercase tracking-wider">LUT 强度</label>
                    <div className="flex items-center gap-4">
                      <input
                        type="range"
                        className="flex-1 accent-[var(--color-accent)]"
                        min="0"
                        max="100"
                        value={settings.lut_intensity}
                        onChange={(e) => updateSetting('lut_intensity', parseInt(e.target.value))}
                        disabled={disabled}
                      />
                      <span className="w-12 text-right font-mono text-sm">{settings.lut_intensity}%</span>
                    </div>
                  </div>
                </div>
              )}

              {activeTab === 'video' && (
                <div className="space-y-4">
                  <div className="space-y-2">
                    <label className="text-xs font-medium text-[var(--color-text-secondary)] uppercase tracking-wider">视频编码器</label>
                    <select
                      className="apple-input"
                      value={settings.video_codec}
                      onChange={(e) => updateSetting('video_codec', e.target.value)}
                      disabled={disabled}
                    >
                      {availableCodecs.video.map(codec => (
                        <option key={codec.name} value={codec.name} disabled={!codec.supported}>
                          {codec.description}
                        </option>
                      ))}
                    </select>
                  </div>

                  <div className="space-y-2">
                    <label className="text-xs font-medium text-[var(--color-text-secondary)] uppercase tracking-wider">音频编码器</label>
                    <select
                      className="apple-input"
                      value={settings.audio_codec}
                      onChange={(e) => updateSetting('audio_codec', e.target.value)}
                      disabled={disabled}
                    >
                      {availableCodecs.audio.map(codec => (
                        <option key={codec.name} value={codec.name} disabled={!codec.supported}>
                          {codec.description}
                        </option>
                      ))}
                    </select>
                  </div>

                  <div className="space-y-2">
                    <label className="text-xs font-medium text-[var(--color-text-secondary)] uppercase tracking-wider">分辨率</label>
                    <select
                      className="apple-input"
                      value={settings.resolution}
                      onChange={(e) => updateSetting('resolution', e.target.value)}
                      disabled={disabled}
                    >
                      {resolutionOptions.map(option => (
                        <option key={option.value} value={option.value}>{option.label}</option>
                      ))}
                    </select>
                  </div>
                </div>
              )}

              {activeTab === 'advanced' && (
                <div className="space-y-4">
                  <div className="space-y-2">
                    <label className="text-xs font-medium text-[var(--color-text-secondary)] uppercase tracking-wider">色彩空间</label>
                    <select
                      className="apple-input"
                      value={settings.color_space}
                      onChange={(e) => updateSetting('color_space', e.target.value)}
                      disabled={disabled}
                    >
                      {colorSpaceOptions.map(option => (
                        <option key={option.value} value={option.value}>{option.label}</option>
                      ))}
                    </select>
                  </div>

                  <div className="pt-2 space-y-3">
                    <label className="flex items-center gap-3 cursor-pointer group">
                      <input
                        type="checkbox"
                        className="w-4 h-4 rounded border-[var(--color-border)] text-[var(--color-accent)] focus:ring-[var(--color-accent)]"
                        checked={settings.hardware_acceleration}
                        onChange={(e) => updateSetting('hardware_acceleration', e.target.checked)}
                        disabled={disabled}
                      />
                      <span className="text-sm text-[var(--color-text-primary)] group-hover:text-[var(--color-accent)] transition-colors">硬件加速</span>
                    </label>

                    <label className="flex items-center gap-3 cursor-pointer group">
                      <input
                        type="checkbox"
                        className="w-4 h-4 rounded border-[var(--color-border)] text-[var(--color-accent)] focus:ring-[var(--color-accent)]"
                        checked={settings.two_pass_encoding}
                        onChange={(e) => updateSetting('two_pass_encoding', e.target.checked)}
                        disabled={disabled}
                      />
                      <span className="text-sm text-[var(--color-text-primary)] group-hover:text-[var(--color-accent)] transition-colors">双通道编码 (更佳质量，更慢速度)</span>
                    </label>

                    <label className="flex items-center gap-3 cursor-pointer group">
                      <input
                        type="checkbox"
                        className="w-4 h-4 rounded border-[var(--color-border)] text-[var(--color-accent)] focus:ring-[var(--color-accent)]"
                        checked={settings.preserve_metadata}
                        onChange={(e) => updateSetting('preserve_metadata', e.target.checked)}
                        disabled={disabled}
                      />
                      <span className="text-sm text-[var(--color-text-primary)] group-hover:text-[var(--color-accent)] transition-colors">保留元数据</span>
                    </label>
                  </div>
                </div>
              )}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="p-4 bg-[var(--color-background)] border-t border-[var(--color-border)] flex justify-between items-center">
          <button
            onClick={resetToDefaults}
            disabled={disabled}
            className="text-xs text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] px-3 py-1.5 rounded hover:bg-[var(--color-border)] transition-colors"
          >
            恢复默认
          </button>
          <button
            onClick={onClose}
            className="apple-button"
          >
            完成
          </button>
        </div>
      </div>
    </div>,
    document.body
  );
};

export default SettingsModal;