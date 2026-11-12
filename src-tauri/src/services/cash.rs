use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use super::{checked_total, validate_id, ServiceError, ServiceResult, JS_MAX_SAFE_INTEGER};
use crate::models::cash::{
    CashBreakdown, CashMovementInput, CashOperation, CashSession, CashSessionDetail,
    CashTimelineEvent, CashTimelineKind, CloseCashInput, OpenCashInput,
};

fn conflict(code: &'static str) -> ServiceError {
    ServiceError::conflict(
        code,
        "La operacion de caja no es valida para el estado actual",
    )
}

fn amount(value: i64, positive: bool) -> ServiceResult<()> {
    if !(0..=JS_MAX_SAFE_INTEGER).contains(&value) || (positive && value == 0) {
        return Err(ServiceError::validation(
            "INVALID_CASH_AMOUNT",
            "Indica centavos enteros dentro del limite permitido",
        ));
    }
    Ok(())
}

fn token(value: &str) -> ServiceResult<&str> {
    if value.trim().is_empty() || value.chars().count() > 128 || value != value.trim() {
        return Err(ServiceError::validation(
            "INVALID_CASH_TOKEN",
            "El token debe tener entre 1 y 128 caracteres sin espacios exteriores",
        ));
    }
    Ok(value)
}

fn operator(connection: &Connection, user_id: i64) -> ServiceResult<String> {
    validate_id(user_id)?;
    connection.query_row("SELECT trim(first_name || ' ' || last_name) || ' <' || email || '>' FROM users WHERE id=?1", [user_id], |r| r.get(0))
        .optional()?.ok_or_else(|| ServiceError::not_found("USER_NOT_FOUND", "Usuario no encontrado"))
}

fn fingerprint(user: i64, kind: &str, session: Option<i64>, value: i64, reason: &str) -> String {
    // Length-delimited canonical JSON avoids ambiguous concatenation; includes audit user.
    serde_json::to_string(&(user, kind, session, value, reason)).expect("serializable cash intent")
}

fn retry(connection: &Connection, token: &str, fingerprint: &str) -> ServiceResult<Option<i64>> {
    let existing: Option<(i64, String)> = connection
        .query_row(
            "SELECT session_id,intent_fingerprint FROM cash_operations WHERE operation_token=?1",
            [token],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    match existing {
        Some((id, saved)) if saved == fingerprint => Ok(Some(id)),
        Some(_) => Err(conflict("CASH_TOKEN_REUSED")),
        None => Ok(None),
    }
}

struct Operation<'a> {
    session: i64,
    kind: &'a str,
    amount: i64,
    reason: &'a str,
    user: i64,
    operator: &'a str,
    token: &'a str,
    fingerprint: &'a str,
}

fn record(connection: &Connection, op: Operation<'_>) -> ServiceResult<()> {
    connection.execute("INSERT INTO cash_operations(session_id,kind,amount,reason,user_id,operator,operation_token,intent_fingerprint) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![op.session,op.kind,op.amount,op.reason,op.user,op.operator,op.token,op.fingerprint])?;
    Ok(())
}

pub fn active_id(connection: &Connection) -> ServiceResult<Option<i64>> {
    Ok(connection
        .query_row(
            "SELECT id FROM cash_sessions WHERE status='open'",
            [],
            |r| r.get(0),
        )
        .optional()?)
}

pub fn require_open(connection: &Connection, id: i64) -> ServiceResult<()> {
    validate_id(id)?;
    if active_id(connection)? != Some(id) {
        return Err(conflict("CASH_SESSION_NOT_OPEN"));
    }
    Ok(())
}

pub fn state(connection: &Connection) -> ServiceResult<Option<CashSession>> {
    active_id(connection)?
        .map(|id| session(connection, id))
        .transpose()
}

fn breakdown(connection: &Connection, id: i64) -> ServiceResult<CashBreakdown> {
    let mut result = CashBreakdown::default();
    // Fold checked integer values rather than SQLite SUM (which can overflow i64).
    let mut statement = connection
        .prepare("SELECT payment_method,total FROM sales WHERE cash_session_id=?1 ORDER BY id")?;
    for row in statement.query_map([id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
        let (method, value) = row?;
        let target = match method.as_str() {
            "efectivo" => &mut result.cash_sales,
            "tarjeta" => &mut result.card_sales,
            _ => &mut result.transfer_sales,
        };
        *target = checked_total(*target, value, "CASH_AMOUNT_OVERFLOW")?;
        result.sale_count = checked_total(result.sale_count, 1, "CASH_AMOUNT_OVERFLOW")?;
    }
    let mut statement = connection.prepare("SELECT kind,amount FROM cash_operations WHERE session_id=?1 AND kind IN ('income','withdraw') ORDER BY id")?;
    for row in statement.query_map([id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
        let (kind, value) = row?;
        let target = if kind == "income" {
            &mut result.income
        } else {
            &mut result.withdrawals
        };
        *target = checked_total(*target, value, "CASH_AMOUNT_OVERFLOW")?;
    }
    Ok(result)
}

fn expected(opening: i64, b: &CashBreakdown) -> ServiceResult<i64> {
    let value = i128::from(opening) + i128::from(b.cash_sales) + i128::from(b.income)
        - i128::from(b.withdrawals);
    if !(0..=i128::from(JS_MAX_SAFE_INTEGER)).contains(&value) {
        return Err(ServiceError::validation(
            "CASH_AMOUNT_OVERFLOW",
            "El esperado excede el limite permitido",
        ));
    }
    Ok(value as i64)
}

pub fn session(connection: &Connection, id: i64) -> ServiceResult<CashSession> {
    validate_id(id)?;
    let (mut s, snapshot): (CashSession, Option<String>) = connection.query_row(
        "SELECT id,status,historical,opening_amount,opened_by,opening_operator,opened_at,closed_by,closing_operator,closed_at,expected_amount,counted_amount,difference,closing_breakdown FROM cash_sessions WHERE id=?1", [id], |r| Ok((CashSession {
            id:r.get(0)?,status:r.get(1)?,historical:r.get(2)?,opening_amount:r.get(3)?,opened_by:r.get(4)?,opening_operator:r.get(5)?,opened_at:r.get(6)?,closed_by:r.get(7)?,closing_operator:r.get(8)?,closed_at:r.get(9)?,expected_amount:r.get(10)?,counted_amount:r.get(11)?,difference:r.get(12)?,breakdown:CashBreakdown::default(),
        }, r.get(13)?))).optional()?.ok_or_else(|| ServiceError::not_found("CASH_SESSION_NOT_FOUND", "Caja no encontrada"))?;
    s.breakdown = match snapshot {
        Some(json) => serde_json::from_str(&json).map_err(|_| {
            ServiceError::internal("INVALID_CASH_SNAPSHOT", "Desglose de caja invalido")
        })?,
        None => breakdown(connection, id)?,
    };
    if s.status == "open" {
        s.expected_amount = Some(expected(
            s.opening_amount.expect("open session opening"),
            &s.breakdown,
        )?);
    }
    Ok(s)
}

pub fn list(connection: &Connection) -> ServiceResult<Vec<CashSession>> {
    let ids = connection
        .prepare("SELECT id FROM cash_sessions ORDER BY id DESC")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<Vec<i64>, _>>()?;
    ids.into_iter().map(|id| session(connection, id)).collect()
}

pub fn detail(connection: &Connection, id: i64) -> ServiceResult<CashSessionDetail> {
    let session = session(connection, id)?;
    let operations = connection.prepare("SELECT id,session_id,kind,amount,reason,user_id,operator,operation_token,created_at FROM cash_operations WHERE session_id=?1 ORDER BY id")?.query_map([id], |r| Ok(CashOperation {
        id:r.get(0)?,session_id:r.get(1)?,kind:r.get(2)?,amount:r.get(3)?,reason:r.get(4)?,user_id:r.get(5)?,operator:r.get(6)?,operation_token:r.get(7)?,created_at:r.get(8)?,
    }))?.collect::<Result<Vec<_>,_>>()?;
    let mut timeline = Vec::new();
    for op in &operations {
        let kind = match op.kind.as_str() {
            "open" => CashTimelineKind::Open,
            "income" => CashTimelineKind::Income,
            "withdraw" => CashTimelineKind::Withdraw,
            "close" => CashTimelineKind::Close,
            _ => {
                return Err(ServiceError::internal(
                    "INVALID_CASH_OPERATION",
                    "Movimiento de caja invalido",
                ))
            }
        };
        timeline.push(CashTimelineEvent {
            event_key: format!("operation:{}", op.id),
            session_id: id,
            kind,
            operation_id: Some(op.id),
            sale_id: None,
            ticket_number: None,
            amount: op.amount,
            reason: op.reason.clone(),
            user_id: op.user_id,
            operator: op.operator.clone(),
            created_at: op.created_at.clone(),
            cash_received: None,
            change_amount: None,
        });
    }
    let mut statement = connection.prepare("SELECT id,total,user_id,cashier_name,cashier_email,sale_date,cash_received,change_amount FROM sales WHERE cash_session_id=?1 AND payment_method='efectivo' ORDER BY id")?;
    let sales = statement.query_map([id], |r| {
        let sale_id: i64 = r.get(0)?;
        Ok(CashTimelineEvent {
            event_key: format!("sale:{sale_id}"),
            session_id: id,
            kind: CashTimelineKind::CashSale,
            operation_id: None,
            sale_id: Some(sale_id),
            ticket_number: Some(format!("POS-{sale_id:08}")),
            amount: r.get(1)?,
            reason: String::new(),
            user_id: r.get(2)?,
            operator: format!("{} <{}>", r.get::<_, String>(3)?, r.get::<_, String>(4)?),
            created_at: r.get(5)?,
            cash_received: r.get(6)?,
            change_amount: r.get(7)?,
        })
    })?;
    timeline.extend(sales.collect::<Result<Vec<_>, _>>()?);
    sort_timeline(&mut timeline);
    Ok(CashSessionDetail {
        session,
        operations,
        timeline,
    })
}

fn sort_timeline(timeline: &mut [CashTimelineEvent]) {
    // Timestamp ties do not reveal commit order across tables. Open first, close
    // last, then lexical source key is a deterministic presentation-only tie-break.
    let rank = |kind| match kind {
        CashTimelineKind::Open => 0,
        CashTimelineKind::Close => 2,
        _ => 1,
    };
    timeline.sort_by(|a, b| {
        (&a.created_at, rank(a.kind), &a.event_key).cmp(&(
            &b.created_at,
            rank(b.kind),
            &b.event_key,
        ))
    });
}

pub fn open(
    connection: &mut Connection,
    user: i64,
    input: OpenCashInput,
) -> ServiceResult<CashSessionDetail> {
    amount(input.opening_amount, false)?;
    let token = token(&input.operation_token)?;
    let fp = fingerprint(user, "open", None, input.opening_amount, "");
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let operator = operator(&tx, user)?;
    if let Some(id) = retry(&tx, token, &fp)? {
        return detail(&tx, id);
    }
    if active_id(&tx)?.is_some() {
        return Err(conflict("CASH_ALREADY_OPEN"));
    }
    tx.execute("INSERT INTO cash_sessions(status,opening_amount,opened_by,opening_operator) VALUES('open',?1,?2,?3)", params![input.opening_amount,user,operator])?;
    let id = tx.last_insert_rowid();
    record(
        &tx,
        Operation {
            session: id,
            kind: "open",
            amount: input.opening_amount,
            reason: "",
            user,
            operator: &operator,
            token,
            fingerprint: &fp,
        },
    )?;
    let result = detail(&tx, id)?;
    tx.commit()?;
    Ok(result)
}

pub fn movement(
    connection: &mut Connection,
    user: i64,
    input: CashMovementInput,
    withdraw: bool,
) -> ServiceResult<CashSessionDetail> {
    amount(input.amount, true)?;
    validate_id(input.session_id)?;
    let token = token(&input.operation_token)?;
    let reason = input.reason.trim();
    if reason.is_empty() || reason.chars().count() > 500 {
        return Err(ServiceError::validation(
            "INVALID_CASH_REASON",
            "El motivo es obligatorio y admite hasta 500 caracteres",
        ));
    }
    let kind = if withdraw { "withdraw" } else { "income" };
    let fp = fingerprint(user, kind, Some(input.session_id), input.amount, reason);
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let operator = operator(&tx, user)?;
    if let Some(id) = retry(&tx, token, &fp)? {
        return detail(&tx, id);
    }
    require_open(&tx, input.session_id)?;
    let before = session(&tx, input.session_id)?;
    if withdraw && input.amount > before.expected_amount.expect("open expected") {
        return Err(conflict("INSUFFICIENT_CASH_BALANCE"));
    }
    record(
        &tx,
        Operation {
            session: input.session_id,
            kind,
            amount: input.amount,
            reason,
            user,
            operator: &operator,
            token,
            fingerprint: &fp,
        },
    )?;
    let result = detail(&tx, input.session_id)?;
    tx.commit()?;
    Ok(result)
}

pub fn close(
    connection: &mut Connection,
    user: i64,
    input: CloseCashInput,
) -> ServiceResult<CashSessionDetail> {
    amount(input.counted_amount, false)?;
    validate_id(input.session_id)?;
    let token = token(&input.operation_token)?;
    let fp = fingerprint(
        user,
        "close",
        Some(input.session_id),
        input.counted_amount,
        "",
    );
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let operator = operator(&tx, user)?;
    if let Some(id) = retry(&tx, token, &fp)? {
        return detail(&tx, id);
    }
    require_open(&tx, input.session_id)?;
    let before = session(&tx, input.session_id)?;
    let expected = before.expected_amount.expect("open expected");
    let snapshot = serde_json::to_string(&before.breakdown).expect("serializable breakdown");
    record(
        &tx,
        Operation {
            session: input.session_id,
            kind: "close",
            amount: input.counted_amount,
            reason: "",
            user,
            operator: &operator,
            token,
            fingerprint: &fp,
        },
    )?;
    tx.execute("UPDATE cash_sessions SET status='closed',closed_by=?1,closing_operator=?2,closed_at=CURRENT_TIMESTAMP,expected_amount=?3,counted_amount=?4,difference=?5,closing_breakdown=?6 WHERE id=?7", params![user,operator,expected,input.counted_amount,input.counted_amount-expected,snapshot,input.session_id])?;
    let result = detail(&tx, input.session_id)?;
    tx.commit()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::migrations,
        models::cart::CheckoutCartInput,
        services::{
            cart::{self, SessionCart},
            sales,
        },
    };

    fn seed(c: &mut Connection) {
        c.pragma_update(None, "foreign_keys", true).unwrap();
        migrations::run(c).unwrap();
        c.execute_batch("INSERT INTO users(first_name,last_name,email,password_hash) VALUES('Ada','POS','ada@local','hash'),('Otro','POS','otro@local','hash');
          INSERT INTO brands(name,name_search_key) VALUES('Marca','marca');
          INSERT INTO categories(name,name_search_key) VALUES('Categoria','categoria');
          INSERT INTO products(name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id) VALUES('Uno','uno',100,100,'UNO','uno',1,1);
          INSERT INTO product_presentations(product_id,name,mode,base_quantity,price,is_default) VALUES(1,'Pieza','fixed',1,100,1);").unwrap();
    }
    fn db() -> Connection {
        let mut c = Connection::open_in_memory().unwrap();
        seed(&mut c);
        c
    }
    fn opening(token: &str, amount: i64) -> OpenCashInput {
        OpenCashInput {
            operation_token: token.into(),
            opening_amount: amount,
        }
    }
    fn closing(id: i64, token: &str, amount: i64) -> CloseCashInput {
        CloseCashInput {
            operation_token: token.into(),
            session_id: id,
            counted_amount: amount,
        }
    }
    fn manual(id: i64, token: &str, amount: i64) -> CashMovementInput {
        CashMovementInput {
            operation_token: token.into(),
            session_id: id,
            amount,
            reason: " Fondo adicional ".into(),
        }
    }
    fn checkout(
        c: &mut Connection,
        session: Option<i64>,
        token: &str,
        method: &str,
    ) -> ServiceResult<crate::models::sale::SaleReceipt> {
        let cart = SessionCart::from([(1, 1)]);
        let fp = cart::get(c, &cart)?.cart_fingerprint;
        sales::checkout(
            c,
            1,
            &cart,
            CheckoutCartInput {
                expected_cash_session_id: session,
                checkout_token: token.into(),
                cart_fingerprint: fp,
                payment_method: method.into(),
                cash_received: if method == "efectivo" {
                    Some("10.00".into())
                } else {
                    None
                },
            },
        )
    }

    #[test]
    fn timeline_projects_net_checkout_cash_without_a_second_ledger_and_survives_retries() {
        let mut c = db();
        c.execute(
            "UPDATE product_presentations SET price=15000 WHERE id=1",
            [],
        )
        .unwrap();
        let id = open(&mut c, 1, opening("open", 0)).unwrap().session.id;
        let cart = SessionCart::from([(1, 1)]);
        let input = CheckoutCartInput {
            expected_cash_session_id: Some(id),
            checkout_token: "net-cash".into(),
            cart_fingerprint: cart::get(&c, &cart).unwrap().cart_fingerprint,
            payment_method: "efectivo".into(),
            cash_received: Some("200.00".into()),
        };
        let receipt = sales::checkout(&mut c, 1, &cart, input).unwrap();
        checkout(&mut c, Some(id), "card", "tarjeta").unwrap();
        checkout(&mut c, Some(id), "transfer", "transferencia").unwrap();
        movement(&mut c, 2, manual(id, "income", 15000), false).unwrap();
        movement(&mut c, 2, manual(id, "withdraw", 5000), true).unwrap();
        let before = session(&c, id).unwrap();
        let projected = detail(&c, id).unwrap();
        assert_eq!(projected.session, before);
        assert_eq!(before.expected_amount, Some(25000));
        assert_eq!(before.breakdown.cash_sales, 15000);
        assert_eq!(before.breakdown.card_sales, 15000);
        assert_eq!(before.breakdown.transfer_sales, 15000);
        assert_eq!(before.breakdown.sale_count, 3);
        assert_eq!(projected.operations.len(), 3);
        assert_eq!(projected.timeline.len(), 4);
        let event = projected
            .timeline
            .iter()
            .find(|e| e.kind == CashTimelineKind::CashSale)
            .unwrap();
        assert_eq!(event.amount, 15000);
        assert_eq!(
            (event.cash_received, event.change_amount),
            (Some(20000), Some(5000))
        );
        assert_eq!(event.sale_id, Some(receipt.sale.id));
        assert_eq!(event.operation_id, None);
        assert_eq!(event.ticket_number.as_deref(), Some("POS-00000001"));
        assert_eq!(event.created_at, receipt.sale.sale_date);
        assert_eq!(event.operator, "Ada POS <ada@local>");
        assert_eq!(event.user_id, receipt.sale.user_id);
        assert!(event.reason.is_empty());
        assert!(projected
            .timeline
            .iter()
            .any(|e| e.event_key == "operation:1"));
        assert_eq!(event.event_key, "sale:1"); // Same numeric ID, different namespace.
        assert!(projected
            .timeline
            .iter()
            .any(|e| e.kind == CashTimelineKind::Income && e.amount == event.amount));
        let closed = close(&mut c, 2, closing(id, "close", 24000)).unwrap();
        assert_eq!(closed.session.expected_amount, before.expected_amount);
        assert_eq!(closed.session.breakdown, before.breakdown);
        assert_eq!(closed.session.difference, Some(-1000));
        assert_eq!(closed.operations.len(), 4);
        assert_eq!(closed.timeline.len(), 5);
        let close_event = closed
            .timeline
            .iter()
            .find(|e| e.kind == CashTimelineKind::Close)
            .unwrap();
        assert_eq!(close_event.amount, 24000);

        // Test timestamp ties deliberately, independent of wall clock speed.
        let mut ties = detail(&c, id).unwrap().timeline;
        for e in &mut ties {
            e.created_at = "2026-01-01 12:00:00".into();
        }
        ties.reverse();
        sort_timeline(&mut ties);
        assert_eq!(
            ties.iter()
                .map(|e| e.event_key.as_str())
                .collect::<Vec<_>>(),
            vec![
                "operation:1",
                "operation:2",
                "operation:3",
                "sale:1",
                "operation:4"
            ]
        );

        let next_id = open(&mut c, 1, opening("next", 0)).unwrap().session.id;
        checkout(&mut c, Some(next_id), "other-session", "tarjeta").unwrap();
        // Another session's cash sale must also stay out of the old timeline.
        c.execute("UPDATE product_presentations SET price=100 WHERE id=1", [])
            .unwrap();
        checkout(&mut c, Some(next_id), "other-cash", "efectivo").unwrap();
        c.execute(
            "UPDATE users SET first_name='Renamed',email='new@local' WHERE id=1",
            [],
        )
        .unwrap();
        let retry = sales::checkout(
            &mut c,
            1,
            &SessionCart::default(),
            CheckoutCartInput {
                expected_cash_session_id: Some(id),
                checkout_token: "net-cash".into(),
                cart_fingerprint: receipt.sale.cart_fingerprint.clone(),
                payment_method: "efectivo".into(),
                cash_received: Some("200.00".into()),
            },
        )
        .unwrap();
        assert_eq!(retry, receipt);
        let recovered = close(&mut c, 2, closing(id, "close", 24000)).unwrap();
        assert_eq!(recovered.session, closed.session);
        assert_eq!(recovered.timeline, closed.timeline);
        assert_eq!(
            serde_json::to_value(&recovered.operations).unwrap(),
            serde_json::to_value(&closed.operations).unwrap()
        );
        assert_eq!(detail(&c, id).unwrap().timeline, closed.timeline);
        assert_eq!(
            movement(&mut c, 2, manual(id, "income", 15000), false)
                .unwrap()
                .timeline,
            closed.timeline
        );
        assert_eq!(recovered.operations.len(), 4);
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM cash_operations WHERE session_id=?1",
                [id],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            4
        );
        let json = serde_json::to_value(&recovered).unwrap();
        let sale_json = json["timeline"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["kind"] == "cash_sale")
            .unwrap();
        assert_eq!(sale_json["amount"], 15000);
        assert!(sale_json["operation_id"].is_null());
        assert!(sale_json.get("operation_token").is_none());
    }

    #[test]
    fn historical_timeline_uses_legacy_sales_without_fabricated_cash_operations() {
        let mut c = Connection::open_in_memory().unwrap();
        migrations::run_to_v8(&mut c).unwrap();
        c.execute_batch("INSERT INTO users(first_name,last_name,email,password_hash) VALUES('Current','Profile','current@local','hash');
            INSERT INTO sales(user_id,cashier_name,cashier_email,checkout_token,cart_fingerprint,sale_date,total,payment_method,cash_received)
            VALUES(1,'Historical Cashier','old@local','old-cash','old-fp','2020-01-01 12:00:00',15000,'efectivo',20000),
            (1,'Historical Cashier','old@local','old-card','card-fp','2020-01-02 12:00:00',200,'tarjeta',NULL);").unwrap();
        migrations::run(&mut c).unwrap();
        let historical = detail(&c, 1).unwrap();
        assert!(historical.session.historical);
        assert_eq!(
            (
                historical.session.opening_amount,
                historical.session.expected_amount,
                historical.session.counted_amount,
                historical.session.difference
            ),
            (None, None, None, None)
        );
        assert!(historical.operations.is_empty());
        assert_eq!(historical.timeline.len(), 1);
        let event = &historical.timeline[0];
        assert_eq!(event.kind, CashTimelineKind::CashSale);
        assert_eq!(event.operator, "Historical Cashier <old@local>");
        assert_eq!(event.created_at, "2020-01-01 12:00:00");
        assert_eq!(event.amount, 15000);
        assert_eq!(
            (event.cash_received, event.change_amount),
            (Some(20000), None)
        );
        assert_eq!(historical.session.breakdown.cash_sales, 15000);
        assert_eq!(historical.session.breakdown.card_sales, 200);
    }

    #[test]
    fn lifecycle_exact_sums_snapshots_global_owner_and_retry_recovery() {
        let mut c = db();
        assert!(list(&c).unwrap().is_empty());
        assert_eq!(
            checkout(&mut c, None, "missing", "tarjeta")
                .unwrap_err()
                .code,
            "CASH_SESSION_REQUIRED"
        );
        assert_eq!(
            checkout(&mut c, Some(1), "missing", "tarjeta")
                .unwrap_err()
                .code,
            "CASH_SESSION_NOT_OPEN"
        );
        let id = open(&mut c, 1, opening("open", 500)).unwrap().session.id;
        assert_eq!(
            open(&mut c, 1, opening("open", 500)).unwrap().session.id,
            id
        );
        assert_eq!(
            open(&mut c, 2, opening("another", 0)).unwrap_err().code,
            "CASH_ALREADY_OPEN"
        );
        assert_eq!(
            open(&mut c, 2, opening("open", 500)).unwrap_err().code,
            "CASH_TOKEN_REUSED"
        );
        assert_eq!(
            movement(&mut c, 1, manual(id, "open", 500), false)
                .unwrap_err()
                .code,
            "CASH_TOKEN_REUSED"
        );
        let sale = checkout(&mut c, Some(id), "cash", "efectivo").unwrap();
        checkout(&mut c, Some(id), "card", "tarjeta").unwrap();
        checkout(&mut c, Some(id), "transfer", "transferencia").unwrap();
        assert_eq!(sale.sale.change_amount, Some(900));
        assert_eq!(
            movement(&mut c, 2, manual(id, "income", 200), false)
                .unwrap()
                .session
                .expected_amount,
            Some(800)
        );
        movement(&mut c, 2, manual(id, "income", 200), false).unwrap();
        assert_eq!(
            movement(&mut c, 1, manual(id, "income", 200), false)
                .unwrap_err()
                .code,
            "CASH_TOKEN_REUSED"
        );
        assert_eq!(
            movement(&mut c, 1, manual(id, "too-much", 801), true)
                .unwrap_err()
                .code,
            "INSUFFICIENT_CASH_BALANCE"
        );
        assert_eq!(
            movement(&mut c, 1, manual(id, "withdraw", 300), true)
                .unwrap()
                .session
                .expected_amount,
            Some(500)
        );
        assert_eq!(
            movement(&mut c, 1, manual(id, "withdraw", 301), true)
                .unwrap_err()
                .code,
            "CASH_TOKEN_REUSED"
        );
        let closed = close(&mut c, 2, closing(id, "close", 450)).unwrap();
        assert_eq!(
            (
                closed.session.expected_amount,
                closed.session.counted_amount,
                closed.session.difference
            ),
            (Some(500), Some(450), Some(-50))
        );
        assert_eq!(
            closed.session.breakdown,
            CashBreakdown {
                cash_sales: 100,
                card_sales: 100,
                transfer_sales: 100,
                sale_count: 3,
                income: 200,
                withdrawals: 300
            }
        );
        assert!(state(&c).unwrap().is_none());
        assert_eq!(
            close(&mut c, 2, closing(id, "close", 450)).unwrap().session,
            closed.session
        );
        assert_eq!(
            close(&mut c, 1, closing(id, "new-close", 450))
                .unwrap_err()
                .code,
            "CASH_SESSION_NOT_OPEN"
        );
        assert_eq!(
            movement(&mut c, 1, manual(id, "new-income", 1), false)
                .unwrap_err()
                .code,
            "CASH_SESSION_NOT_OPEN"
        );
        assert_eq!(
            checkout(&mut c, Some(id), "new-sale", "tarjeta")
                .unwrap_err()
                .code,
            "CASH_SESSION_NOT_OPEN"
        );
        crate::services::products::set_active(&mut c, 1, false).unwrap();
        let new_id = open(&mut c, 1, opening("new-open", 0)).unwrap().session.id;
        assert_ne!(new_id, id);
        assert_eq!(
            open(&mut c, 1, opening("open", 500)).unwrap().session,
            closed.session
        );
        let recovered_movement = movement(&mut c, 2, manual(id, "income", 200), false).unwrap();
        assert_eq!(recovered_movement.session, closed.session);
        assert_eq!(recovered_movement.operations.len(), 4);
        assert_eq!(
            movement(&mut c, 2, manual(new_id, "income", 200), false)
                .unwrap_err()
                .code,
            "CASH_TOKEN_REUSED"
        );
        let mut changed_reason = manual(id, "income", 200);
        changed_reason.reason = "Otra intencion".into();
        assert_eq!(
            movement(&mut c, 2, changed_reason, false).unwrap_err().code,
            "CASH_TOKEN_REUSED"
        );
        assert!(c
            .execute(
                "UPDATE sale_details SET product_name='Changed' WHERE sale_id=?1",
                [sale.sale.id]
            )
            .is_err());
        assert_eq!(
            sales::view(
                &c,
                crate::models::sale::SalesViewFilters {
                    cash_session_id: Some(id),
                    ..Default::default()
                }
            )
            .unwrap()
            .stats
            .sale_count,
            3
        );
        assert!(sales::view(
            &c,
            crate::models::sale::SalesViewFilters {
                cash_session_id: Some(new_id),
                ..Default::default()
            }
        )
        .unwrap()
        .items
        .is_empty());
        let recovered = sales::checkout(
            &mut c,
            1,
            &SessionCart::default(),
            CheckoutCartInput {
                expected_cash_session_id: None,
                checkout_token: "cash".into(),
                cart_fingerprint: sale.sale.cart_fingerprint.clone(),
                payment_method: "efectivo".into(),
                cash_received: Some("10.00".into()),
            },
        )
        .unwrap();
        assert_eq!(recovered, sale);
        assert_eq!(
            checkout(&mut c, Some(new_id), "cash", "efectivo")
                .unwrap_err()
                .code,
            "CHECKOUT_TOKEN_REUSED"
        );
        assert_eq!(
            checkout(&mut c, Some(id), "stale", "tarjeta")
                .unwrap_err()
                .code,
            "CASH_SESSION_NOT_OPEN"
        );
        c.execute("UPDATE users SET first_name='Renamed' WHERE id=2", [])
            .unwrap();
        assert_eq!(session(&c, id).unwrap(), closed.session);
        assert_eq!(list(&c).unwrap().len(), 2);
        assert!(c
            .execute(
                "UPDATE cash_sessions SET counted_amount=0 WHERE id=?1",
                [id]
            )
            .is_err());
        let sale_before =
            serde_json::to_value(sales::history_detail(&c, sale.sale.id).unwrap()).unwrap();
        let error = c
            .execute(
                "UPDATE sales SET cashier_name='Changed cashier' WHERE id=?1",
                [sale.sale.id],
            )
            .unwrap_err();
        assert!(error.to_string().contains("CASH_SESSION_IMMUTABLE"));
        assert_eq!(
            serde_json::to_value(sales::history_detail(&c, sale.sale.id).unwrap()).unwrap(),
            sale_before
        );
        assert!(c
            .execute(
                "UPDATE sales SET cash_session_id=?1 WHERE id=?2",
                params![new_id, sale.sale.id]
            )
            .is_err());
    }

    #[test]
    fn validation_limits_overflow_and_rollback() {
        let mut c = db();
        for value in [-1, JS_MAX_SAFE_INTEGER + 1] {
            assert_eq!(
                open(&mut c, 1, opening("invalid", value)).unwrap_err().code,
                "INVALID_CASH_AMOUNT"
            );
        }
        assert_eq!(
            open(&mut c, 99, opening("unknown", 0)).unwrap_err().code,
            "USER_NOT_FOUND"
        );
        for value in ["", "  ", &"a".repeat(129)] {
            assert_eq!(
                open(&mut c, 1, opening(value, 0)).unwrap_err().code,
                "INVALID_CASH_TOKEN"
            );
        }
        let id = open(&mut c, 1, opening(&"t".repeat(128), JS_MAX_SAFE_INTEGER))
            .unwrap()
            .session
            .id;
        for value in [-1, 0, JS_MAX_SAFE_INTEGER + 1] {
            assert_eq!(
                movement(&mut c, 1, manual(id, "invalid", value), false)
                    .unwrap_err()
                    .code,
                "INVALID_CASH_AMOUNT"
            );
        }
        for reason in ["".into(), " \n\t ".into(), "a".repeat(501)] {
            let mut input = manual(id, "reason", 1);
            input.reason = reason;
            assert_eq!(
                movement(&mut c, 1, input, false).unwrap_err().code,
                "INVALID_CASH_REASON"
            );
        }
        assert_eq!(
            movement(&mut c, 1, manual(id, "overflow", 1), false)
                .unwrap_err()
                .code,
            "CASH_AMOUNT_OVERFLOW"
        );
        assert_eq!(
            checkout(&mut c, Some(id), "overflow-sale", "efectivo")
                .unwrap_err()
                .code,
            "CASH_AMOUNT_OVERFLOW"
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM sales", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            c.query_row("SELECT stock FROM products WHERE id=1", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            100
        );
        let mut input = manual(id, "limit", 1);
        input.reason = "a".repeat(500);
        movement(&mut c, 1, input, true).unwrap();
        movement(&mut c, 1, manual(id, "overflow", 1), false).unwrap(); // failed intent did not consume token
        for value in [-1, JS_MAX_SAFE_INTEGER + 1] {
            assert_eq!(
                close(&mut c, 1, closing(id, "bad-close", value))
                    .unwrap_err()
                    .code,
                "INVALID_CASH_AMOUNT"
            );
        }
        assert!(serde_json::from_str::<OpenCashInput>(
            r#"{"operation_token":"x","opening_amount":1.5}"#
        )
        .is_err());
        assert!(serde_json::from_str::<CashMovementInput>(
            r#"{"operation_token":"x","session_id":1,"amount":1,"reason":"x","user_id":2}"#
        )
        .is_err());
    }

    #[test]
    fn concurrent_duplicate_movements_and_withdrawals_do_not_double_spend() {
        use std::sync::{Arc, Barrier};
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("duplicates.db");
        {
            let mut c = Connection::open(&path).unwrap();
            seed(&mut c);
            open(&mut c, 1, opening("open", 100)).unwrap();
        }
        for duplicate in [true, false] {
            let barrier = Arc::new(Barrier::new(2));
            let workers: Vec<_> = (0..2)
                .map(|i| {
                    let path = path.clone();
                    let b = barrier.clone();
                    std::thread::spawn(move || {
                        let mut c = Connection::open(path).unwrap();
                        c.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
                        b.wait();
                        if duplicate {
                            movement(&mut c, 1, manual(1, "same-income", 100), false)
                        } else {
                            movement(&mut c, 1, manual(1, &format!("withdraw-{i}"), 150), true)
                        }
                    })
                })
                .collect();
            let results: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
            if duplicate {
                assert!(results.iter().all(|r| r.is_ok()));
            } else {
                assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
                assert_eq!(
                    results.iter().find_map(|r| r.as_ref().err()).unwrap().code,
                    "INSUFFICIENT_CASH_BALANCE"
                );
            }
        }
        let c = Connection::open(path).unwrap();
        let detail = detail(&c, 1).unwrap();
        assert_eq!(detail.session.expected_amount, Some(50));
        assert_eq!(detail.operations.len(), 3);
        assert!(c.execute("INSERT INTO cash_sessions(status,opening_amount,opened_by,opening_operator) VALUES('open',0,1,'bypass')",[]).is_err());
    }

    #[test]
    fn single_open_race_reopen_and_close_vs_checkout_are_serialized() {
        use std::sync::{Arc, Barrier};
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("cash.db");
        {
            let mut c = Connection::open(&path).unwrap();
            seed(&mut c);
        }
        let barrier = Arc::new(Barrier::new(2));
        let workers: Vec<_> = (0..2)
            .map(|i| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let mut c = Connection::open(path).unwrap();
                    c.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
                    barrier.wait();
                    open(&mut c, 1, opening(&format!("race-{i}"), 0))
                })
            })
            .collect();
        let results: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        assert_eq!(
            results.iter().find_map(|r| r.as_ref().err()).unwrap().code,
            "CASH_ALREADY_OPEN"
        );
        let mut c = Connection::open(&path).unwrap();
        c.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
        let id = state(&c).unwrap().unwrap().id;
        let barrier = Arc::new(Barrier::new(2));
        let b = barrier.clone();
        let p = path.clone();
        let closer = std::thread::spawn(move || {
            let mut c = Connection::open(p).unwrap();
            c.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
            b.wait();
            close(&mut c, 1, closing(id, "race-close", 0)).unwrap()
        });
        barrier.wait();
        let sale = checkout(&mut c, Some(id), "race-sale", "efectivo");
        let closed = closer.join().unwrap();
        match sale {
            Ok(sale) => {
                assert_eq!(closed.session.expected_amount, Some(100));
                assert_eq!(
                    checkout(&mut c, Some(id), "race-sale", "efectivo").unwrap(),
                    sale
                );
            }
            Err(e) => {
                assert_eq!(e.code, "CASH_SESSION_NOT_OPEN");
                assert_eq!(closed.session.expected_amount, Some(0));
            }
        }
        drop(c);
        let c = Connection::open(&path).unwrap();
        assert_eq!(session(&c, id).unwrap(), closed.session);
    }
}
