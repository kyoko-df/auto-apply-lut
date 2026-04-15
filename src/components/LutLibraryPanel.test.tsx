import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import LutLibraryPanel from './LutLibraryPanel';

const invokeMock = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
  convertFileSrc: (path: string) => `asset://${path}`,
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}));

describe('LutLibraryPanel', () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it('在资料同步完成前显示明确的占位提示', () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === 'list_lut_library') {
        return [];
      }
      if (command === 'remember_lut_files') {
        return new Promise(() => {});
      }
      if (command === 'generate_lut_preview') {
        return '/tmp/preview.png';
      }
      return [];
    });

    render(
      <LutLibraryPanel
        activeVideoPath={null}
        selectedLutPaths={['/tmp/sample.cube']}
        onSelectedLutPathsChange={vi.fn()}
      />
    );

    expect(screen.getAllByText('正在读取资料').length).toBeGreaterThan(0);
    expect(screen.getByText('同步中')).toBeInTheDocument();
    expect(screen.getByText('正在读取文件资料')).toBeInTheDocument();
  });

  it('在同步失败时向界面暴露错误信息', async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === 'list_lut_library') {
        return [];
      }
      if (command === 'remember_lut_files') {
        throw new Error('同步 LUT 资料失败');
      }
      if (command === 'generate_lut_preview') {
        return '/tmp/preview.png';
      }
      return [];
    });

    render(
      <LutLibraryPanel
        activeVideoPath={null}
        selectedLutPaths={['/tmp/sample.cube']}
        onSelectedLutPathsChange={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('同步 LUT 资料失败')).toBeInTheDocument();
    });
  });
});
