import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import App from './App';

const invokeMock = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock('./components/FileUpload', () => ({
  default: () => <div data-testid="file-upload">FileUpload</div>,
}));

vi.mock('./components/VideoPreview', () => ({
  default: () => <div data-testid="video-preview">VideoPreview</div>,
}));

vi.mock('./components/ProcessingStatus', () => ({
  default: () => <div data-testid="processing-status">ProcessingStatus</div>,
}));

vi.mock('./components/SettingsModal', () => ({
  default: ({ isOpen }: { isOpen: boolean }) =>
    isOpen ? <div data-testid="settings-modal">SettingsModal</div> : null,
}));

vi.mock('./components/LutLibraryPanel', () => ({
  default: () => <div data-testid="lut-library-panel">LUT 资料库面板</div>,
}));

describe('App', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue({});
    window.localStorage.clear();
  });

  it('通过 header 按钮打开 LUT 资料库弹窗', () => {
    render(<App />);

    expect(screen.queryByRole('dialog', { name: 'LUT 资料库' })).not.toBeInTheDocument();
    expect(screen.queryByTestId('lut-library-panel')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'LUT 资料库' }));

    expect(screen.getByRole('dialog', { name: 'LUT 资料库' })).toBeInTheDocument();
    expect(screen.getByTestId('lut-library-panel')).toBeInTheDocument();
  });
});
