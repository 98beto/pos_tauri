import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import App from "./App";
import { invokeMock } from "./test/setup";
import { emptyCart, user } from "./test/fixtures";

beforeEach(() => {
  invokeMock.mockReset().mockImplementation((command: string) => {
    const routes: Record<string, unknown> = {
      owner_exists: true,
      current_user: user,
      get_cart: emptyCart,
      list_categories: [],
    };
    if (!(command in routes)) return Promise.reject(new Error(`Unexpected command: ${command}`));
    return Promise.resolve(routes[command]);
  });
});

describe("App smoke", () => {
  it("boots the real application tree into the POS", async () => {
    render(<App />);
    expect(await screen.findByRole("heading", { name: "Carrito actual" })).toBeTruthy();
    expect(screen.getByText(`${user.first_name} ${user.last_name}`)).toBeTruthy();
    expect(invokeMock).toHaveBeenCalledWith("owner_exists");
    expect(invokeMock).toHaveBeenCalledWith("current_user");
    expect(invokeMock).toHaveBeenCalledWith("get_cart");
  });
});
