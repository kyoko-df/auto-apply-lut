import { fireEvent, render, screen, waitFor } from '@testing-library/react';
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

  const createLibraryItem = (overrides: Record<string, unknown> = {}) => ({
    path: '/tmp/a.cube',
    name: 'a.cube',
    size: 128,
    lut_type: 'ThreeDimensional',
    format: 'CUBE',
    category: '3D LUT',
    is_valid: true,
    updated_at: new Date().toISOString(),
    error_message: null,
    ...overrides
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

  it('未选中 LUT 时禁用批量转换按钮', async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === 'list_lut_library') {
        return [createLibraryItem()];
      }
      if (command === 'generate_lut_preview') {
        return '/tmp/preview.png';
      }
      return [];
    });

    render(
      <LutLibraryPanel
        activeVideoPath={null}
        selectedLutPaths={[]}
        onSelectedLutPathsChange={vi.fn()}
      />
    );

    expect(await screen.findByRole('button', { name: '批量转换' })).toBeDisabled();
  });

  it('选中 LUT 后可以打开批量转换弹窗', async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === 'list_lut_library') {
        return [createLibraryItem()];
      }
      if (command === 'remember_lut_files') {
        return [createLibraryItem()];
      }
      if (command === 'generate_lut_preview') {
        return '/tmp/preview.png';
      }
      return [];
    });

    render(
      <LutLibraryPanel
        activeVideoPath={null}
        selectedLutPaths={['/tmp/a.cube']}
        onSelectedLutPathsChange={vi.fn()}
      />
    );

    fireEvent.click(await screen.findByRole('button', { name: '批量转换' }));

    expect(screen.getByText('批量转换 LUT')).toBeInTheDocument();
    expect(screen.getByText('已选 1 个文件')).toBeInTheDocument();
  });

  it('混合维度 LUT 时禁止批量转换', async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === 'list_lut_library') {
        return [
          createLibraryItem(),
          createLibraryItem({
            path: '/tmp/b.lut',
            name: 'b.lut',
            lut_type: 'OneDimensional',
            format: 'LUT',
            category: '1D LUT'
          })
        ];
      }
      if (command === 'remember_lut_files') {
        return [];
      }
      if (command === 'generate_lut_preview') {
        return '/tmp/preview.png';
      }
      return [];
    });

    render(
      <LutLibraryPanel
        activeVideoPath={null}
        selectedLutPaths={['/tmp/a.cube', '/tmp/b.lut']}
        onSelectedLutPathsChange={vi.fn()}
      />
    );

    fireEvent.click(await screen.findByRole('button', { name: '批量转换' }));

    expect(
      screen.getByText('当前选中项包含不同维度的 LUT，无法批量转换到同一目标格式')
    ).toBeInTheDocument();
  });

  it('批量转换后展示成功与失败摘要', async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === 'list_lut_library') {
        return [createLibraryItem()];
      }
      if (command === 'remember_lut_files') {
        return [createLibraryItem()];
      }
      if (command === 'generate_lut_preview') {
        return '/tmp/preview.png';
      }
      if (command === 'batch_convert_luts') {
        return {
          success_count: 1,
          failure_count: 0,
          results: [
            {
              source_path: '/tmp/a.cube',
              target_path: '/tmp/a.converted.csp',
              success: true,
              error_message: null
            }
          ]
        };
      }
      return [];
    });

    render(
      <LutLibraryPanel
        activeVideoPath={null}
        selectedLutPaths={['/tmp/a.cube']}
        onSelectedLutPathsChange={vi.fn()}
      />
    );

    fireEvent.click(await screen.findByRole('button', { name: '批量转换' }));
    fireEvent.change(screen.getByLabelText('目标格式'), { target: { value: 'Csp' } });
    fireEvent.click(screen.getByRole('button', { name: '开始转换' }));

    expect(await screen.findByText('成功 1 个，失败 0 个')).toBeInTheDocument();
    expect(screen.getByText('/tmp/a.converted.csp')).toBeInTheDocument();
  });
});
