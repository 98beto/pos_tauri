# Caja backend v15 (bloque 3)

## Contrato IPC y TypeScript

Exports en `src/api/cash.ts`, tipos en `src/types/cash.ts`. Todos los comandos
requieren autenticación local. No reciben `user_id`: el usuario autenticado se
usa exclusivamente como auditoría; no existe caja por usuario ni autorización
por propietario de una sesión.

| Export TS | Comando | Argumentos invoke | Resultado |
|---|---|---|---|
| `getCashState()` | `get_cash_state` | ninguno | `CashSession \| null` |
| `listCashSessions()` | `list_cash_sessions` | ninguno | `CashSession[]` (id descendente) |
| `getCashSessionDetail(sessionId)` | `get_cash_session_detail` | `{ sessionId }` | `CashSessionDetail` |
| `openCashSession(input)` | `open_cash_session` | `{ input }` | `CashSessionDetail` |
| `cashIncome(input)` | `cash_income` | `{ input }` | `CashSessionDetail` |
| `cashWithdraw(input)` | `cash_withdraw` | `{ input }` | `CashSessionDetail` |
| `closeCashSession(input)` | `close_cash_session` | `{ input }` | `CashSessionDetail` |

Los campos dentro de `input` y de las respuestas son **snake_case**:

```ts
OpenCashInput = { operation_token: string; opening_amount: number }
CashMovementInput = {
  operation_token: string; session_id: number; amount: number; reason: string
}
CloseCashInput = {
  operation_token: string; session_id: number; counted_amount: number
}
CashSessionDetail = { session: CashSession; operations: CashOperation[] }
CashSession = {
  id: number; status: "open" | "closed"; historical: boolean;
  opening_amount: number | null; opened_by: number | null;
  opening_operator: string | null; opened_at: string;
  closed_by: number | null; closing_operator: string | null;
  closed_at: string | null; expected_amount: number | null;
  counted_amount: number | null; difference: number | null;
  breakdown: CashBreakdown
}
CashBreakdown = {
  cash_sales: number; card_sales: number; transfer_sales: number;
  sale_count: number; income: number; withdrawals: number
}
CashOperation = {
  id: number; session_id: number; kind: "open" | "income" | "withdraw" | "close";
  amount: number; reason: string; user_id: number; operator: string;
  operation_token: string; created_at: string
}
```

- Importes: **centavos enteros**, no texto decimal ni pesos, entre 0 y
  `Number.MAX_SAFE_INTEGER` (9007199254740991). Ingreso/retiro estrictamente > 0.
  Diferencia puede ser negativa dentro del mismo límite absoluto. Agregados
  fuera del límite fallan y revierten la operación completa, incluido checkout.
- Motivo: `trim()` Unicode, obligatorio, máximo 500 caracteres Unicode después
  de trim. Se almacena y compara la forma normalizada.
- Token: 1–128 caracteres Unicode, sin espacios exteriores. Generar una vez por
  intención y conservar en reintentos. Namespace global para operaciones de caja
  (apertura/ingreso/retiro/cierre); independiente del token de checkout.
- Operador: snapshot `Nombre Apellido <email>`. Fechas SQLite UTC en formato
  `YYYY-MM-DD HH:MM:SS`; no son preferencias locales de presentación.
- `expected_amount` se calcula en caja abierta; en caja cerrada normal proviene
  del snapshot persistido. `counted_amount` y `difference` son null estando abierta.
- `operations` ordenadas por id ascendente. Incluye apertura/cierre; para éstos
  `reason` es `""` y `amount` es apertura/contado respectivamente.

## Checkout e intenciones pendientes

`CheckoutCartInput.expected_cash_session_id?: number | null` es **obligatorio
para toda venta nueva**, aunque se mantiene opcional en deserialización/TS para
recuperar intenciones antiguas ya confirmadas. UI debe capturar el id de caja al
crear la intención pendiente y guardarlo junto al token, fingerprint y pago.
Nunca sustituir automáticamente ese id por otra caja al reintentar.

Dentro de `BEGIN IMMEDIATE`, se busca primero el token de checkout guardado y
se validan usuario, carrito/fingerprint y pago con las reglas existentes. Si el
input incluye sesión, debe coincidir con la original; omitirla sólo admite la
recuperación de una venta ya existente. Un token confirmado devuelve su recibo
original incluso con la caja cerrada, otra caja abierta o el producto archivado.
No cambia sesión ni stock. Un token nuevo sin id falla `CASH_SESSION_REQUIRED`;
un id que no es la caja abierta falla `CASH_SESSION_NOT_OPEN`, nunca cobra bajo
otra caja silenciosamente.

`Sale`, `SalesViewRow` y `SaleHistoryDetail` ahora incluyen `cash_session_id:
number` obligatorio; también está en `SaleReceipt.sale`. `getSalesView` acepta
`filters.cash_session_id?: number` junto a búsqueda y método de pago y calcula
estadísticas sólo sobre el conjunto filtrado. `listSales` sigue sin argumentos.

## Idempotencia y cierre

El fingerprint canónico de caja incluye usuario, clase, sesión (salvo apertura,
que aún no tiene id), importe y motivo normalizado. El token queda asociado a su
sesión al guardar. Mismo token/distinta intención falla `CASH_TOKEN_REUSED`;
mismo token/misma intención no escribe ni duplica operaciones, incluso después
del cierre. La respuesta de reintento es el **detalle actual de la sesión original**,
no una copia de la respuesta de apertura/movimiento. Un cierre ya guardado sí
mantiene idéntico snapshot.

Una única caja abierta global se garantiza con índice parcial. Apertura,
movimientos, cierre y checkout usan transacciones `Immediate` cortas, sin esperar
red/UI. Cierre y venta se serializan: o la venta entra en el snapshot o se rechaza
sin cambios de venta/stock/movimientos. Logout/reinicio no cierra la caja.

Esperado = apertura + total de ventas `efectivo` + ingresos − retiros. No usa
`cash_received`; cambio ya está descontado en el total. Tarjeta/transferencia se
reportan por separado. Retiro no puede superar el esperado actual. Cierre guarda
esperado, contado, diferencia, desglose, fecha y operador; después es inmutable,
al igual que sus operaciones y las ventas/detalles asociados.

## Errores relevantes

Se conserva el envelope `AppError` (`category`, `code`, `message`, `field`).

- `SESSION_REQUIRED` (authentication).
- `INVALID_CASH_AMOUNT`, `INVALID_CASH_TOKEN`, `INVALID_CASH_REASON`,
  `CASH_AMOUNT_OVERFLOW`, `CASH_SESSION_REQUIRED`, `INVALID_ID` (validation).
- `CASH_ALREADY_OPEN`, `CASH_SESSION_NOT_OPEN`, `CASH_TOKEN_REUSED`,
  `INSUFFICIENT_CASH_BALANCE`, `CHECKOUT_TOKEN_REUSED` (conflict).
- `CASH_SESSION_NOT_FOUND`, `USER_NOT_FOUND` (not_found).
- Se mantienen los errores previos de carrito, producto archivado, pago y stock.
- Inputs con claves desconocidas, fracciones para campos enteros o tipos inválidos
  son rechazados por deserialización IPC antes del servicio.

## Migración y QA

001–014 permanecen intactas. 015 añade tablas/índices/guards y FK en ventas sin
reconstruir los snapshots existentes. La columna FK se añade nullable por las
restricciones de ALTER TABLE de SQLite; el backfill, trigger obligatorio en INSERT,
inmutabilidad de asociación y validador de arranque impiden ventas sin sesión.

Si hay ventas anteriores, todas se enlazan a sesión 1 histórica cerrada, fechas
min/max de las ventas, sin inventar apertura, esperado, contado, diferencia ni
operador. Sus desgloses son sumas verificables de ventas inmutables, no un arqueo
fabricado. Si no hay ventas, no se crea historia ficticia y la primera apertura
real puede ser id 1.

El migrador productivo acepta fuentes canónicas v8/v12/v13/v14, con backup
validado y protocolo durable de recuperación existente. Conserva proyecciones
anteriores y añade comparación de `is_active` para v14. La referencia canónica
v15 verifica también índices, triggers y FK; verifica ausencia de ventas sin caja.

`pnpm qa:migration-v8-v13` sigue funcionando con sus nombres históricos de
entrada/artefactos; ahora prueba las cuatro rutas a v15. Entrada adicional:
`node scripts/run-v15-migration-qa.mjs`. Los reportes declaran target v15 y la ruta
v14 incluye producto archivado, historial, backup, reapertura y restore durable.
