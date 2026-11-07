import type { CashSession } from "../types/cash";

export const openCash: CashSession = {
  id: 2, status: "open", historical: false, opening_amount: 1000,
  opened_by: 1, opening_operator: "Ada <ada@example.com>", opened_at: "2026-01-01 10:00:00",
  closed_by: null, closing_operator: null, closed_at: null,
  expected_amount: 1000, counted_amount: null, difference: null,
  breakdown: { cash_sales: 0, card_sales: 0, transfer_sales: 0, sale_count: 0, income: 0, withdrawals: 0 },
};
