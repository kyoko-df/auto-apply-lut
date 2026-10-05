import { useLayoutEffect, useRef, useState, type ReactNode } from "react";

/** Fixed-height rows keep very large workspaces responsive without a dependency. */
export default function WindowedList<T>({ items, rowHeight, itemKey, children, label }: {
  items: T[]; rowHeight: number; itemKey: (item: T) => string;
  children: (item: T, index: number) => ReactNode; label: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [viewport, setViewport] = useState({ top: 0, height: 600 });
  const virtual = items.length > 80;
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const update = () => setViewport({ top: el.scrollTop, height: el.clientHeight || 600 });
    update();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(update);
    observer.observe(el);
    return () => observer.disconnect();
  }, [items.length]);
  const start = virtual ? Math.max(0, Math.min(items.length - 1, Math.floor(viewport.top / rowHeight)) - 5) : 0;
  const end = virtual ? Math.min(items.length, start + Math.ceil(viewport.height / rowHeight) + 12) : items.length;
  return <div ref={ref} className="windowed-list" aria-label={label}
    onScroll={e => setViewport({ top: e.currentTarget.scrollTop, height: e.currentTarget.clientHeight })}>
    <div style={virtual ? { height: items.length * rowHeight, position: "relative" } : undefined}>
      {items.slice(start, end).map((item, i) => <div key={itemKey(item)} className={virtual ? "windowed-row" : undefined}
        style={virtual ? { position: "absolute", top: (start + i) * rowHeight, width: "100%", height: rowHeight } : undefined}>
        {children(item, start + i)}
      </div>)}
    </div>
  </div>;
}
