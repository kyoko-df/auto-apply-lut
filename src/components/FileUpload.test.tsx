import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import FileUpload from './FileUpload';

const openMock = vi.fn();
const invokeMock = vi.fn();

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: (...args: unknown[]) => openMock(...args),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

describe('FileUpload', () => {
  beforeEach(() => {
    openMock.mockReset();
    invokeMock.mockReset();
    openMock.mockResolvedValue(null);
  });

  it('不再暴露 VLT 作为可选 LUT 格式', async () => {
    const { container } = render(
      <FileUpload
        onVideoSelect={vi.fn()}
        onActiveVideoChange={vi.fn()}
        onLutSelect={vi.fn()}
      />
    );

    const lutInput = container.querySelector('input[accept*=".cube"]');
    expect(lutInput).not.toBeNull();
    expect(lutInput?.getAttribute('accept')).not.toContain('.vlt');

    fireEvent.click(screen.getByText('选择LUT文件'));

    expect(openMock).toHaveBeenCalledWith(
      expect.objectContaining({
        filters: [
          expect.objectContaining({
            extensions: expect.not.arrayContaining(['vlt']),
          }),
        ],
      })
    );
  });
});
