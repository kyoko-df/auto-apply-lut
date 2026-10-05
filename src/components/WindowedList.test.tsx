import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import WindowedList from "./WindowedList";

const items = Array.from({ length: 1000 }, (_, i) => ({
  id: String(i),
  title: `素材 ${i}`,
}));
const row = (item: (typeof items)[number]) => <button>{item.title}</button>;

describe("windowed media lists", () => {
  it("bounds rendered rows and uses absolute row positions when scrolling a large library", () => {
    render(
      <WindowedList
        items={items}
        rowHeight={60}
        itemKey={(item) => item.id}
        label="素材列表"
      >
        {row}
      </WindowedList>,
    );
    expect(screen.getAllByRole("button").length).toBeLessThan(30);
    expect(screen.getByRole("button", { name: "素材 0" })).toBeVisible();
    const list = screen.getByLabelText("素材列表");
    Object.defineProperty(list, "clientHeight", { value: 600 });
    fireEvent.scroll(list, { target: { scrollTop: 6000 } });
    expect(
      screen.queryByRole("button", { name: "素材 0" }),
    ).not.toBeInTheDocument();
    const target = screen.getByRole("button", { name: "素材 100" });
    expect(target.parentElement).toHaveStyle({ top: "6000px", height: "60px" });
    expect(screen.getAllByRole("button").length).toBeLessThan(30);
  });

  it("renders all rows for small lists and recovers when a scrolled large list becomes short", () => {
    const view = (source: typeof items) => (
      <WindowedList
        items={source}
        rowHeight={60}
        itemKey={(item) => item.id}
        label="素材列表"
      >
        {row}
      </WindowedList>
    );
    const { rerender } = render(view(items));
    const list = screen.getByLabelText("素材列表");
    Object.defineProperty(list, "clientHeight", { value: 600 });
    fireEvent.scroll(list, { target: { scrollTop: 50000 } });
    rerender(view(items.slice(0, 5)));
    expect(screen.getAllByRole("button")).toHaveLength(5);
    expect(
      screen.getByRole("button", { name: "素材 0" }).parentElement,
    ).not.toHaveClass("windowed-row");
  });
});
