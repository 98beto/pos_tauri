/** Monetary numbers are exact integer cents; historical reconciliation is null. */
export interface CashBreakdown {
  cash_sales: number;
  card_sales: number;
  transfer_sales: number;
  sale_count: number;
  income: number;
  withdrawals: number;
}
export interface CashSession {
  id: number;
  status: "open" | "closed";
  historical: boolean;
  opening_amount: number | null;
  opened_by: number | null;
  opening_operator: string | null;
  opened_at: string;
  closed_by: number | null;
  closing_operator: string | null;
  closed_at: string | null;
  expected_amount: number | null;
  counted_amount: number | null;
  difference: number | null;
  breakdown: CashBreakdown;
}
export interface CashOperation {
  id: number;
  session_id: number;
  kind: "open" | "income" | "withdraw" | "close";
  amount: number;
  reason: string;
  user_id: number;
  operator: string;
  operation_token: string;
  created_at: string;
}
export interface CashSessionDetail {
  session: CashSession;
  operations: CashOperation[];
  timeline: CashTimelineEvent[];
}
/** Derived read model, not a token-bearing persisted cash operation. Amount is
 * net sale total, opening fund, manual amount or counted closing cash in cents. */
export interface CashTimelineEvent {
  event_key: string;
  session_id: number;
  kind: "open" | "income" | "withdraw" | "cash_sale" | "close";
  operation_id: number | null;
  sale_id: number | null;
  ticket_number: string | null;
  amount: number;
  reason: string;
  user_id: number;
  operator: string;
  created_at: string;
  cash_received: number | null;
  change_amount: number | null;
}
export interface OpenCashInput {
  operation_token: string;
  opening_amount: number;
}
export interface CashMovementInput {
  operation_token: string;
  session_id: number;
  amount: number;
  reason: string;
}
export interface CloseCashInput {
  operation_token: string;
  session_id: number;
  counted_amount: number;
}
