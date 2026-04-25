import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import SettingsModal from './SettingsModal';

const invokeMock = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const baseProps = {
  isOpen: true,
  settings: {
    output_format: 'mp4',
    video_codec: 'libx264',
    audio_codec: 'aac',
    quality_preset: 'balanced',
    resolution: 'original',
    fps: null,
    bitrate: 'auto',
    lut_intensity: 100,
    lut_error_strategy: 'StopOnError' as const,
    color_space: 'rec709',
    hardware_acceleration: false,
    two_pass_encoding: false,
    preserve_metadata: true,
    output_directory: '',
  },
  onClose: vi.fn(),
  onSettingsChange: vi.fn(),
};

describe('SettingsModal', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    (window as Window & { __TAURI_INTERNALS__?: object }).__TAURI_INTERNALS__ = {};
  });

  it('打开弹窗后加载系统信息与缓存大小', async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === 'get_available_codecs') {
        return { video_codecs: [], audio_codecs: [] };
      }
      if (command === 'get_gpu_info') {
        return [];
      }
      if (command === 'check_hardware_acceleration') {
        return { available: false, supported_codecs: [], recommended_settings: [] };
      }
      if (command === 'get_system_info') {
        return {
          cpu_usage: 0,
          memory_usage: 42.5,
          total_memory: 17179869184,
          available_memory: 8589934592,
          disk_usage: [],
          cpu_count: 10,
          system_name: 'macOS',
          system_version: '15.0',
        };
      }
      if (command === 'get_cache_size') {
        return 1048576;
      }
      return null;
    });

    render(<SettingsModal {...baseProps} />);

    await waitFor(() => {
      expect(invokeMock.mock.calls.some(([command]) => command === 'get_system_info')).toBe(true);
      expect(invokeMock.mock.calls.some(([command]) => command === 'get_cache_size')).toBe(true);
    });

    expect(await screen.findByText('系统与缓存')).toBeInTheDocument();
    expect(screen.getByText('macOS 15.0')).toBeInTheDocument();
    expect(screen.getByText(/缓存大小:\s*1.0 MB/)).toBeInTheDocument();
  });

  it('清理缓存后刷新缓存大小', async () => {
    let cacheReads = 0;

    invokeMock.mockImplementation(async (command: string) => {
      if (command === 'get_available_codecs') {
        return { video_codecs: [], audio_codecs: [] };
      }
      if (command === 'get_gpu_info') {
        return [];
      }
      if (command === 'check_hardware_acceleration') {
        return { available: false, supported_codecs: [], recommended_settings: [] };
      }
      if (command === 'get_system_info') {
        return {
          cpu_usage: 0,
          memory_usage: 12,
          total_memory: 17179869184,
          available_memory: 8589934592,
          disk_usage: [],
          cpu_count: 8,
          system_name: 'macOS',
          system_version: '15.0',
        };
      }
      if (command === 'get_cache_size') {
        cacheReads += 1;
        return cacheReads === 1 ? 3145728 : 0;
      }
      if (command === 'clear_cache') {
        return 'Cache cleared successfully';
      }
      return null;
    });

    render(<SettingsModal {...baseProps} />);

    expect(await screen.findByText(/缓存大小:\s*3.0 MB/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '清理缓存' }));

    await waitFor(() => {
      expect(invokeMock.mock.calls.some(([command]) => command === 'clear_cache')).toBe(true);
    });

    expect(await screen.findByText(/缓存大小:\s*0 B/)).toBeInTheDocument();
  });

  it('加载日志列表并展示选中文件内容', async () => {
    invokeMock.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === 'get_available_codecs') {
        return { video_codecs: [], audio_codecs: [] };
      }
      if (command === 'get_gpu_info') {
        return [];
      }
      if (command === 'check_hardware_acceleration') {
        return { available: false, supported_codecs: [], recommended_settings: [] };
      }
      if (command === 'get_system_info') {
        return {
          cpu_usage: 0,
          memory_usage: 12,
          total_memory: 17179869184,
          available_memory: 8589934592,
          disk_usage: [],
          cpu_count: 8,
          system_name: 'macOS',
          system_version: '15.0',
        };
      }
      if (command === 'get_cache_size') {
        return 0;
      }
      if (command === 'get_log_files') {
        return ['app.log', 'worker.log'];
      }
      if (command === 'read_log_file') {
        expect(args).toEqual({ fileName: 'app.log' });
        return '2026-04-24 10:00:00 INFO boot ok';
      }
      return null;
    });

    render(<SettingsModal {...baseProps} />);

    fireEvent.click(await screen.findByRole('button', { name: '加载日志' }));

    expect(await screen.findByRole('button', { name: 'app.log' })).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'app.log' }));

    expect(await screen.findByText('2026-04-24 10:00:00 INFO boot ok')).toBeInTheDocument();
  });

  it('浏览器预览模式下显示系统诊断降级提示', async () => {
    delete (window as Window & { __TAURI_INTERNALS__?: object }).__TAURI_INTERNALS__;

    render(<SettingsModal {...baseProps} />);

    expect(
      await screen.findByText('当前环境不支持系统诊断（浏览器预览模式）')
    ).toBeInTheDocument();
  });

  it('日志读取失败时保留文件列表并显示错误', async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === 'get_available_codecs') {
        return { video_codecs: [], audio_codecs: [] };
      }
      if (command === 'get_gpu_info') {
        return [];
      }
      if (command === 'check_hardware_acceleration') {
        return { available: false, supported_codecs: [], recommended_settings: [] };
      }
      if (command === 'get_system_info') {
        return {
          cpu_usage: 0,
          memory_usage: 12,
          total_memory: 17179869184,
          available_memory: 8589934592,
          disk_usage: [],
          cpu_count: 8,
          system_name: 'macOS',
          system_version: '15.0',
        };
      }
      if (command === 'get_cache_size') {
        return 0;
      }
      if (command === 'get_log_files') {
        return ['app.log'];
      }
      if (command === 'read_log_file') {
        throw new Error('read_failed');
      }
      return null;
    });

    render(<SettingsModal {...baseProps} />);

    fireEvent.click(await screen.findByRole('button', { name: '加载日志' }));
    fireEvent.click(await screen.findByRole('button', { name: 'app.log' }));

    expect(await screen.findByText('日志内容读取失败')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'app.log' })).toBeInTheDocument();
  });
});
