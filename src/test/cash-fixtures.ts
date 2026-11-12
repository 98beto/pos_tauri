import type { CashSession, CashTimelineEvent } from "../types/cash";

export const openCash: CashSession = {
  id: 2, status: "open", historical: false, opening_amount: 1000,
  opened_by: 1, opening_operator: "Ada <ada@example.com>", opened_at: "2026-01-01 10:00:00",
  closed_by: null, closing_operator: null, closed_at: null,
  expected_amount: 1000, counted_amount: null, difference: null,
  breakdown: { cash_sales: 0, card_sales: 0, transfer_sales: 0, sale_count: 0, income: 0, withdrawals: 0 },
};

export const cashSaleEvent: CashTimelineEvent = {
  event_key: "sale:1", session_id: 2, kind: "cash_sale", operation_id: null,
  sale_id: 1, ticket_number: "POS-00000001", amount: 15000, reason: "",
  user_id: 1, operator: "Ada <ada@example.com>", created_at: "2026-01-01 12:00:00",
  cash_received: 20000, change_amount: 5000,
};
export const cashIncomeEvent: CashTimelineEvent = {
  event_key: "operation:4", session_id: 1, kind: "income", operation_id: 4,
  sale_id: null, ticket_number: null, amount: 10, reason: "Cambio",
  user_id: 1, operator: "Otro operador <otro@example.com>", created_at: "2026-01-01 12:00:00",
  cash_received: null, change_amount: null,
};
