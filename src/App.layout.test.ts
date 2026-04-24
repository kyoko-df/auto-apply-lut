import { describe, expect, it } from 'vitest';
import appCss from './App.css?raw';

describe('App layout styles', () => {
  it('不再为桌面网格布局写死 840px 最小高度', () => {
    expect(appCss).not.toContain('min-height: 840px;');
  });
});
