import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useViewPreference } from "./useViewPreference";

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
});

describe("useViewPreference", () => {
  it("uses the default for corrupt values and persists valid selections", () => {
    localStorage.setItem("layout", "grid");
    const { result } = renderHook(() => useViewPreference("layout", "list"));
    expect(result.current[0]).toBe("list");
    act(() => result.current[1]("cards"));
    expect(localStorage.getItem("layout")).toBe("cards");
  });

  it("continues to work when storage access is denied", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => { throw new Error("denied"); });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw new Error("denied"); });
    const { result } = renderHook(() => useViewPreference("layout", "cards"));
    expect(result.current[0]).toBe("cards");
    act(() => result.current[1]("list"));
    expect(result.current[0]).toBe("list");
  });
});
