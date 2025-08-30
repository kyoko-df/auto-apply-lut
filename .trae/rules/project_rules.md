# 项目规则（补充：苹果风格设计美学）

本项目采用“苹果风格（Apple aesthetic）”的设计规范。除通用代码规范外，UI/UX 必须遵循以下原则与检查项。详尽原则见 <mcfile name="苹果风格设计规范.md" path="d:\Develop\video\auto-apply-lut\.trae\ai_docs\苹果风格设计规范.md"></mcfile>。

一、统一的视觉语言
- 字体：优先使用系统字体（macOS: SF Pro；Windows: Segoe UI / 微软雅黑；中文优先苹方）。
- 色彩：主色 #007AFF；状态色按规范执行（成功 #34C759、警告 #FF9F0A、错误 #FF3B30）。
- 圆角：按钮/输入 10，卡片 12，对话框 14–16。
- 阴影：低对比柔和（示例 0 8px 24px rgba(0,0,0,0.08)）。

二、布局与间距
- 采用 8pt 网格体系，常用间距 8/12/16/24/32。
- 容器内边距：卡片 16–20；弹窗 24–28；最小可点击面积 44×44。

三、动效与交互
- 动效时长 120–240ms，缓动 cubic-bezier(0.25,0.8,0.25,1)。
- 动效克制，尽量淡入淡出 + 微缩放（0.96→1.0）。
- 所有可点击元素必须具备 hover、active、focus 三态反馈，禁用态降低对比度。

四、磨砂与层级
- 优先使用 frosted 质感（backdrop-filter: blur(20–30px) + 半透明背景），Win10 不支持时退化为 elevated 实色背景。
- 合理使用层级：base / elevated / frosted，尽量使用留白而非粗分隔线。

五、组件落地（Tailwind 建议类）
- 统一 tokens 类（避免散落颜色值）：
  - btn-primary: 统一主按钮风格
  - card: 统一卡片风格
  - frosted: 统一磨砂层风格
- 新增或修改组件时，应优先复用上述 tokens 类或在 tokens 基础上扩展。

六、可访问性
- 文本对比度 ≥ 4.5:1；键盘焦点可见；尊重 prefers-reduced-motion。

七、PR 检查清单（必须满足）
- 是否遵循 <mcfile name="苹果风格设计规范.md" path="d:\Develop\video\auto-apply-lut\.trae\ai_docs\苹果风格设计规范.md"></mcfile> 中的字体、颜色、圆角、阴影、间距、动效标准？
- 是否使用统一的 Tailwind tokens 类（btn-primary、card、frosted 等）而非硬编码颜色？
- 是否对 hover/active/focus/disabled 等状态进行了样式处理并验证？
- 是否在暗色模式下进行了对比度和可读性检查？
- Win10 环境下是否验证了 backdrop-filter 的降级效果？

八、例外与变更
- 如需突破规范，需在 PR 中说明理由与设计稿；变更规范需同步更新 <mcfile name="苹果风格设计规范.md" path="d:\Develop\video\auto-apply-lut\.trae\ai_docs\苹果风格设计规范.md"></mcfile> 并通知全体成员。