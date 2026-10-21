# Compras, conteo fisico y retiro del costo acumulado

**Estado:** implementacion local completada; gates operativos pendientes.

**Esquema de origen en produccion:** SQLite 12.

**Esquema objetivo:** SQLite 13.

## Motivo del reemplazo

La captura inicial de costo de SQLite 12 permitio registrar precios con una base distinta a la esperada. En produccion existieron casos donde la interfaz esperaba un precio por gramo y el usuario capturo un precio por kilogramo. El costo acumulado resultante no es confiable y no existe informacion suficiente para corregirlo automaticamente.

El equipo operativo confirmo que:

- SQLite 13 no se ha usado con datos reales; solo existe en desarrollo y pruebas.
- Los costos historicos de SQLite 12 no deben migrarse.
- El sistema dejara de valorar inventario y de calcular costo de ventas.
- El precio de compra se conservara solamente como dato de cada compra nueva.
- Los ajustes se realizaran mediante conteo fisico en unidad base.
- Proveedor, precio y factura pertenecen exclusivamente a una compra.

Por ello, la migracion 013 puede reemplazarse antes de publicarse. No se creara SQLite 14.

## Objetivo

Convertir Inventario en un modulo operativo simple, separado en dos tareas:

1. Registrar una compra con cantidad, precio, proveedor y factura.
2. Ajustar existencias indicando el stock fisico contado.

El resultado esperado es:

- El inventario controla cantidades fisicas, no valoracion contable.
- Una compra aumenta stock y conserva su propio precio e importe documental.
- Un ajuste establece el stock contado y calcula la diferencia automaticamente.
- Un ajuste no solicita precio, proveedor ni factura.
- Una venta descuenta stock sin calcular costo de venta.
- No existen costo acumulado, costo promedio, valor a costo ni valoracion manual.
- Corregir una compra cambia solamente el precio y el importe de esa compra.
- La migracion elimina intencionalmente todos los costos historicos de SQLite 12.
- Stock, ventas cobradas, cantidades, relaciones y metadatos no financieros se conservan literalmente.
- La actualizacion crea y valida un backup SQLite 12 antes de eliminar datos financieros.

## Principios de producto

### Operacion antes que formulario

No se usara un formulario generico con un selector de tipo de movimiento. La interfaz presentara acciones distintas:

- `Registrar compra`.
- `Ajustar existencias`.

Cada accion mostrara solamente los campos que le corresponden.

### Stock como fuente de verdad

El stock se persiste en la unidad base del producto:

- `piece`: piezas enteras.
- `gram`: gramos enteros.
- `milliliter`: mililitros enteros.
- `centimeter`: centimetros enteros.

Las presentaciones siguen sirviendo para vender y registrar compras. Los ajustes por conteo se capturan siempre en unidad base para evitar ambiguedad.

### Sin valoracion de inventario

El sistema no intentara inferir:

- Costo promedio.
- Costo acumulado.
- Valor del inventario.
- Costo retirado por una venta.
- Margen bruto.

El precio de una compra es un atributo documental de esa compra, no una entrada a un saldo contable derivado.

## Alcance

Este plan modifica:

- Esquema SQLite 13 y migracion desde SQLite 8 y 12.
- Apertura, backup, manifiestos y restauracion.
- Contratos Rust, TypeScript e IPC de inventario.
- Registro y correccion de compras.
- Ajustes mediante conteo fisico.
- Checkout y movimientos de venta.
- Productos, estadisticas e historial de inventario.
- Intents locales de operaciones pendientes.
- Pruebas frontend, backend y migracion.
- QA, release y rollback.
- Documentacion historica de los planes de costo reemplazados.

## Fuera de alcance

- Valoracion contable del inventario.
- Costo promedio ponderado.
- Costo de ventas o margen bruto.
- Reportes de utilidad.
- Editar cantidad, producto, presentacion, proveedor, factura o fecha de una compra confirmada.
- Borrar compras, ajustes, ventas o movimientos fisicos. La migracion 013 elimina como excepcion confirmada los movimientos historicos de valoracion, porque no representan un cambio de stock y su unica finalidad era establecer el saldo contable retirado.
- Cambiar la unidad base de un producto existente.
- Reabrir o modificar totales comerciales de ventas cerradas.
- Auditoria de versiones anteriores del precio corregido.
- Unidades base con decimales.
- Paquetes `fixed` para productos medidos.
- Importar costos historicos de SQLite 12.
- Intentar reparar costos antiguos mediante conversiones heuristicas.

## Decisiones confirmadas

### Version del esquema

SQLite 13 no fue publicado ni usado con datos reales. Se reemplazara la migracion 013 existente.

La politica de apertura sera:

```text
0 nuevo -> crear esquema 13
8       -> backup -> migraciones 9..13 -> validar 13
12      -> backup -> migracion 13 -> validar 13
13      -> validar y abrir
9..11   -> rechazar como version intermedia
otra    -> rechazar como incompatible
```

Las bases SQLite 13 locales creadas por la implementacion descartada se consideran datos de desarrollo y deben recrearse. No se agregara compatibilidad con esa forma local del esquema 13.

### Datos financieros historicos

La migracion desde SQLite 12 eliminara intencionalmente:

- `products.inventory_cost_cents`.
- `sale_details.movement_cost_cents`.
- `inventory_movements.unit_price_cents` historico.
- `inventory_movements.movement_cost_cents` historico.
- `inventory_movements.inventory_cost_before_cents`.
- `inventory_movements.inventory_cost_after_cents`.
- Todos los movimientos historicos con `reason = valoracion` o `type = valoracion`.

No se copiaran esos valores a columnas nuevas, tablas auxiliares, notas ni manifiestos.

El backup SQLite 12 previo a la migracion sera la unica copia conservada por el proceso de actualizacion. Un rollback restaura ese archivo completo, incluidos sus campos financieros antiguos.

### Datos que deben preservarse

La migracion conservara literalmente:

- Usuarios.
- Marcas y categorias.
- Productos y unidad base.
- Stock por producto.
- Presentaciones, conversiones, precios al cliente y codigos de barras.
- Ventas, detalles, precios comerciales, descuentos, pagos y totales cobrados.
- Cantidades capturadas y cantidades base.
- Relaciones entre venta, detalle y movimiento.
- Proveedor, documento y nota historicos como metadatos de movimientos.
- Tokens, fingerprints y timestamps que sigan formando parte del esquema objetivo.
- IDs de todas las filas retenidas y secuencias SQLite.

La eliminacion de movimientos historicos de valoracion es la unica excepcion deliberada a la preservacion de filas e IDs. Sus IDs no se reutilizan y la secuencia de `inventory_movements` conserva el high-water mark de origen, incluso si la valoracion eliminada tenia el ID mas alto.

Todos los metadatos y sus claves de busqueda en filas v1/v2 retenidas se copian literalmente, aunque proveedor o documento aparezcan en una operacion donde las reglas v3 ya no los admitirian. La exclusividad de proveedor y factura para compras aplica solo a escrituras v3.

### Presentaciones medidas

Se mantienen las decisiones ya implementadas:

| Unidad base | Modo para datos nuevos | Base estandar |
| --- | --- | --- |
| `piece` | `fixed` | 1 pieza |
| `gram` | `measured` | 1000 g = 1 kg |
| `milliliter` | `measured` | 1000 ml = 1 L |
| `centimeter` | `measured` | 100 cm = 1 m |

La migracion no reinterpreta presentaciones historicas. Una conversion historica usada permanece bloqueada.

## Modelo objetivo

### Producto

`products` conserva:

- Identidad y metadatos.
- Unidad base inmutable.
- Stock actual en unidad base.
- Precio comercial legado sincronizado con la presentacion predeterminada.
- Marca, categoria y timestamps.

`products` no contiene:

- Costo acumulado.
- Costo promedio persistido.
- Precio de compra.
- Valor de inventario.

Un producto nuevo se crea con:

```text
stock = 0
```

No crea un movimiento inicial.

### Detalle de venta

`sale_details` conserva toda la dimension comercial:

- Precio de catalogo.
- Precio aplicado.
- Cantidad capturada y cantidad base.
- Presentacion y conversion historicas.
- Subtotal de linea.

`sale_details` no contiene costo retirado de inventario.

### Movimiento de inventario

`inventory_movements` conserva la identidad y trazabilidad fisica:

- Producto y snapshots de busqueda.
- Origen, motivo y tipo.
- Presentacion y snapshots de conversion cuando corresponde.
- Cantidad capturada y cantidad base.
- Stock antes y despues.
- Venta y detalle relacionados.
- Proveedor, factura/referencia y nota segun la operacion.
- Token de operacion y fingerprint.
- Timestamps.

Agrega nombres financieros exclusivos para compras nuevas:

```text
purchase_unit_price_cents
purchase_total_cents
```

No usa nombres genericos como `unit_price_cents` o `movement_cost_cents` para datos nuevos.

### Versiones de movimiento

Los movimientos historicos v1 y v2 no relacionados con valoracion se conservan como historia fisica, sin las columnas de costo eliminadas. Sus campos nuevos de compra quedan en `NULL` y no se reinterpretan como compras nuevas.

Todos los movimientos nuevos, manuales o generados por venta, usan `record_version = 3`. Las ventas v1/v2 se admiten solo como historia migrada; el runtime no crea nuevas filas de venta v1/v2.

#### Compra v3

```text
origin = manual
reason = compra
type = entrada
purchase_unit_price_cents > 0
purchase_total_cents > 0
supplier no vacio
document_reference no vacio
snapshots de presentacion completos y coherentes cuando se usa presentacion
operation_token no vacio y unico
intent_fingerprint no vacio
stock_after = stock_before + base_quantity
```

Una compra cuyo importe redondeado half-up sea cero se rechaza. El usuario debe capturar una cantidad o precio que produzca al menos un centavo.

#### Ajuste v3

```text
origin = manual
reason = ajuste
type = entrada | salida
presentation_id = NULL
purchase_unit_price_cents = NULL
purchase_total_cents = NULL
supplier = NULL
document_reference = NULL
note no vacia
operation_token no vacio y unico
intent_fingerprint no vacio
stock_after = stock contado
base_quantity = abs(stock_after - stock_before)
```

Un conteo igual al stock vigente no crea movimiento.

#### Venta

La venta conserva su version y snapshots comerciales actuales. El movimiento registra exclusivamente la salida fisica, usa `origin = sale`, `reason = venta`, `type = salida`, exige `sale_id` y `sale_detail_id` coherentes, deja los campos de compra en `NULL` y cumple `stock_before = stock_after + base_quantity`.

#### Matriz exhaustiva de movimientos

| Version y origen | Destino en SQLite 13 | Reglas principales |
| --- | --- | --- |
| v1 `initial_stock` | Retener | `ajuste/entrada`, sin relaciones de venta, snapshots parciales permitidos como en origen, campos de compra en `NULL` |
| v1 `manual` | Retener salvo valoracion | `compra/entrada` o `ajuste/entrada|salida`, sin relaciones de venta, metadatos literales, campos de compra en `NULL` |
| v1 `sale` | Retener | `venta/salida`, relaciones de venta coherentes, campos de compra en `NULL` |
| v2 `initial_stock` | Retener | Misma forma fisica y continuidad de stock del origen, campos de compra en `NULL` |
| v2 `manual` compra/ajuste | Retener | Forma, snapshots, token, fingerprint y metadatos no financieros literales; campos de compra en `NULL` |
| v2 `manual` valoracion | Eliminar | Excepcion intencional; no cambia stock y solo representa el saldo contable retirado |
| v2 `sale` | Retener | `venta/salida`, detalle y venta completos, snapshots coherentes, campos de compra en `NULL` |
| v3 `manual` compra | Crear | Forma de compra v3 definida arriba |
| v3 `manual` conteo | Crear | Forma de ajuste v3 definida arriba |
| v3 `sale` | Crear | Forma de venta definida arriba; nunca contiene datos de compra |

Reglas comunes del `CHECK` objetivo:

- Ninguna fila retenida o nueva admite `reason = valoracion` o `type = valoracion`.
- v1 conserva la nulabilidad historica permitida por SQLite 12.
- v2 retenido conserva la forma estructural no financiera de SQLite 12.
- v2/v3 con presentacion exigen snapshots todos nulos o todos completos y coherentes.
- Stock y cantidades v2/v3 son enteros no negativos y cumplen su ecuacion de entrada o salida.
- El conteo v3 no admite presentacion ni snapshots de presentacion.
- Solo la compra v3 admite `purchase_unit_price_cents` y `purchase_total_cents`.
- Combinaciones de origen, motivo y tipo no enumeradas se rechazan.
- Se agregan pruebas SQL directas para cada rama valida e invalida.

## Registro de compra

### Campos

Una compra solicita obligatoriamente:

- Producto.
- Forma de captura o presentacion.
- Cantidad.
- Precio de compra.
- Proveedor.
- Factura o referencia.

La nota es opcional.

### Interpretacion del precio

La interfaz debe mostrar la base del precio sin ambiguedad:

```text
fixed:
base_quantity = entered_quantity * presentation.base_quantity
purchase_total = entered_quantity * purchase_unit_price

measured:
base_quantity = entered_quantity
purchase_total = round_half_up(
  entered_quantity * purchase_unit_price / presentation.base_quantity
)

captura directa:
base_quantity = entered_quantity
purchase_total = entered_quantity * purchase_unit_price_per_base
```

Ejemplos de etiquetas:

- `Precio de compra por paquete de 12 piezas`.
- `Precio de compra por 1 kg`.
- `Precio de compra por 1 L`.
- `Precio de compra por 1 m`.
- `Precio de compra por gramo` para captura directa.

Antes de confirmar se muestra:

- Stock actual.
- Cantidad capturada.
- Cantidad base agregada.
- Stock resultante.
- Precio y base de precio.
- Importe total de la compra.
- Proveedor y factura.

### Transaccion e idempotencia

La confirmacion usa una transaccion `IMMEDIATE` y valida nuevamente:

- Producto y presentacion vigentes.
- Conversion.
- Stock actual.
- Precio normalizado.
- Proveedor, factura y nota.
- Fingerprint cotizado.

El token de operacion evita duplicar una compra si la respuesta se pierde.

## Ajuste por conteo fisico

### Flujo

El usuario selecciona un producto e ingresa el stock real contado en unidad base.

La interfaz muestra:

- Unidad de captura.
- Stock registrado actualmente.
- Stock contado.
- Diferencia calculada.
- Resultado `Entrada` o `Salida`.
- Motivo obligatorio.

No muestra:

- Presentaciones.
- Precio de compra.
- Proveedor.
- Factura.
- Campos de costo.

### Calculo

```text
difference = counted_stock - current_stock

difference > 0 -> ajuste de entrada
difference < 0 -> ajuste de salida
difference = 0 -> rechazar sin escritura

base_quantity = abs(difference)
stock_before = current_stock
stock_after = counted_stock
```

El conteo y la diferencia deben ser enteros seguros para JavaScript.

La confirmacion usa una transaccion `IMMEDIATE`. Si el stock cambia despues de cotizar, exige una nueva cotizacion.

## Correccion del precio de compra

### Elegibilidad

Solo una compra nueva y completa puede corregirse:

```text
origin = manual
reason = compra
type = entrada
record_version = 3
purchase_unit_price_cents IS NOT NULL
purchase_total_cents IS NOT NULL
```

Las compras historicas de SQLite 12 no conservan precio y no son elegibles.

### Comportamiento

La correccion modifica solamente:

- `purchase_unit_price_cents`.
- `purchase_total_cents` derivado de la cantidad y conversion originales.

Permanecen inmutables:

- Producto.
- Presentacion y conversion.
- Cantidad capturada y cantidad base.
- Stock antes y despues.
- Proveedor.
- Factura.
- Nota.
- Fecha.
- Token original.

No existe replay porque ninguna venta ni movimiento posterior depende del precio de compra.

La operacion mantiene dos pasos:

1. Cotizar el nuevo importe sin escribir.
2. Confirmar dentro de una transaccion `IMMEDIATE` con fingerprint del movimiento vigente.

El fingerprint canonico incluye:

- ID del movimiento.
- Precio anterior cotizado.
- Precio nuevo solicitado.
- Cantidad, presentacion y conversion originales.
- Proveedor, factura, nota y timestamp vigentes.
- `updated_at` del movimiento.

La confirmacion tiene solamente dos rutas validas:

1. Si el precio vigente coincide con el precio anterior cotizado, valida todo el fingerprint y aplica el cambio.
2. Si el precio vigente ya coincide con el precio nuevo, reconstruye la misma transicion `anterior -> nuevo`; si los campos inmutables coinciden, devuelve exito sin escribir.

Cualquier otro estado exige una nueva cotizacion. No se agrega token ni tabla de auditoria para la correccion.

## Checkout y ventas

Checkout mantiene exclusivamente la dimension comercial y fisica:

- Valida stock agregado por producto.
- Descuenta stock.
- Conserva precio de catalogo, precio aplicado, descuento, pago y total.
- Crea detalles y movimientos relacionados.
- No consulta ni modifica costos de inventario.
- No distribuye costos entre lineas.

Varias lineas del mismo producto se agregan para validar stock, como en la implementacion actual.

## Interfaz objetivo

### Vista principal

La cabecera de Inventario ofrece:

- Boton primario `Registrar compra`.
- Boton secundario `Ajustar existencias`.

Se eliminan:

- El boton generico `Registrar movimiento`.
- El selector de operacion.
- La opcion `Establecer costo acumulado`.
- Tarjetas de valor a costo y costo desconocido.
- Filtros de estado de costo.

Las estadisticas muestran por separado:

- Cantidad de productos.
- Existencia en piezas.
- Existencia por peso.
- Existencia por volumen.
- Existencia por longitud.

No se suman unidades incompatibles.

### Detalle de producto

El detalle funciona como centro operativo del producto:

- `Registrar compra` con producto preseleccionado.
- `Ajustar existencias` con producto preseleccionado.
- Stock actual y ultimo movimiento.
- Historial fisico y documental.

Se eliminan costo promedio y valor a costo.

### Historial

Una compra muestra:

- Cantidad y presentacion.
- Precio de compra y base del precio.
- Importe total.
- Proveedor y factura.
- Nota.

Un ajuste muestra:

- Stock anterior.
- Stock contado.
- Diferencia.
- Entrada o salida.
- Motivo.

Una venta muestra:

- Cantidad retirada.
- Ticket relacionado.

En movil, los movimientos deben mostrarse como tarjetas o resumen expandible. Las acciones importantes no pueden depender de desplazarse hasta la ultima columna de una tabla ancha.

## Intents locales

Se introduce una nueva version con discriminante explicito:

```text
kind = purchase | stock_count
```

### Compra pendiente

Conserva:

- Usuario.
- Token.
- Producto y presentacion.
- Cantidad.
- Precio.
- Proveedor.
- Factura.
- Nota.
- Fingerprint.

### Conteo pendiente

Conserva:

- Usuario.
- Token.
- Producto.
- Stock contado.
- `adjustmentReason`.
- Fingerprint.

No contiene campos comerciales.

`adjustmentReason` es el nombre TypeScript e intent. El DTO Rust/IPC usa `adjustment_reason`. Al persistir se normaliza en `inventory_movements.note` y `note_search_key`; la columna enum `reason` permanece fija en `ajuste`. Fingerprints, respuestas de historial, errores de campo y pruebas usan esta misma correspondencia.

Los intents de versiones anteriores nunca se confirman directamente bajo las reglas nuevas. Se cargan solo como contexto y exigen volver a capturar y cotizar. Si representan una respuesta perdida, el backend consulta primero el token para evitar duplicar una operacion ya aplicada.

## Migracion 013

Se reemplazara nuevamente, antes de publicar:

```text
src-tauri/migrations/013_measured_presentations_centimeters.sql
```

Las migraciones `001-012` no se modifican.

### Reconstrucciones

SQLite 13 reconstruira, en orden seguro:

1. `products` para retirar costo acumulado y admitir centimetros.
2. `product_presentations` para restablecer su FK.
3. `sale_details` para retirar costo de movimiento y admitir centimetros.
4. `inventory_movements` para retirar costos antiguos, agregar campos de compra y admitir centimetros.

La copia usara listas explicitas de columnas. No se permite `SELECT *`.

### Validacion pre y post

La validacion usa proyecciones distintas por version de origen.

#### Ruta SQLite 12 a 13

`v12-before` y `v13-after-v12` comparan literalmente todos los campos retenidos de las filas no eliminadas:

- IDs y secuencias.
- Stock.
- Productos y presentaciones.
- Ventas, pagos, descuentos y totales.
- Cantidades capturadas y base.
- Relaciones.
- Proveedores, documentos y notas.
- Tokens y timestamps.

Los movimientos de valoracion se fotografian y cuentan antes de migrar solamente para comprobar su eliminacion intencional. No participan en la igualdad de filas retenidas.

#### Ruta SQLite 8 a 13

`v8-before` compara literalmente las columnas nativas de SQLite 8. `v13-after-v8` valida por separado los valores que 009-012 generan deterministicamente, incluyendo presentaciones, unidades, snapshots y backfills comerciales. No se exige igualdad para columnas que no existian en v8.

Las cuatro proyecciones son SQL explicito e independiente:

- `v8-before`.
- `v13-after-v8`.
- `v12-before`.
- `v13-after-v12`.

No se reutiliza una forma unica que contenga columnas eliminadas.

Los campos financieros eliminados no participan en la igualdad post-migracion. En su lugar se valida que:

- Las columnas antiguas no existan.
- Ningun valor antiguo haya sido copiado a los campos nuevos de compra.
- Las compras historicas tengan campos nuevos de compra en `NULL`.
- No permanezca ningun movimiento de valoracion.
- La secuencia de movimientos conserve el high-water mark de origen.

### Backup y rollback

Antes de cualquier escritura:

- Preflight read-only de SQLite 8 o 12.
- Backup consistente mediante API SQLite.
- Validacion del backup.
- Marcador durable pendiente.
- Migracion.
- Validacion post-migracion.
- Manifiesto completado.

El manifiesto completado declara como minimo:

- ID unico del intento.
- Version de origen y version objetivo.
- Ruta del backup validado.
- Validacion del backup completada.
- Validacion canonica del esquema objetivo completada.
- Comparacion source-specific completada.
- Verificacion de eliminacion financiera y de valoraciones completada.

La publicacion atomica del manifiesto completado funciona como atestacion durable; no se promete un hash que la implementacion no verifique.

Un rollback restaura exactamente la version y contenido logico de origen: SQLite 8 para una fuente v8 y SQLite 12 para una fuente v12. En produccion se espera la segunda ruta; esa restauracion recupera los costos antiguos porque pertenecen al backup, aunque no formen parte de SQLite 13.

## Contratos e IPC

Se reemplaza el DTO manual generico por contratos separados:

- `QuotePurchaseInput` / `CreatePurchaseInput`.
- `QuoteStockCountInput` / `CreateStockCountInput`.
- `QuotePurchasePriceCorrectionInput` / `ConfirmPurchasePriceCorrectionInput`.

Rust es autoridad de dominio:

- Una compra rechaza proveedor, factura o precio vacios.
- Un conteo rechaza cualquier campo comercial.
- Una compra respeta modo y conversion de presentacion.
- Un conteo no acepta presentacion.
- Todos los calculos usan enteros comprobados y limite seguro de JavaScript.

`QuoteStockCountInput` y `CreateStockCountInput` contienen `adjustment_reason`; no exponen el enum persistido `reason`, precio, proveedor, factura ni presentacion.

## Orden de implementacion

### Bloque 1: contratos y plan

- Marcar el modelo de costo acumulado como reemplazado.
- Definir DTO separados de compra y conteo.
- Definir campos persistidos exclusivos de compra.
- Definir nueva version de intents.

### Bloque 2: migracion 013 destructiva controlada

- Reescribir 013 desde SQLite 12.
- Eliminar costos historicos.
- Preservar datos no financieros.
- Adaptar validacion canonica, snapshots, backup y restauracion.
- Invalidar bases locales SQLite 13 con la forma descartada.

### Bloque 3: compras

- Implementar cotizacion y confirmacion.
- Exigir precio, proveedor y factura.
- Mantener presentaciones medidas y bases claras.
- Guardar precio e importe solo en la compra.
- Implementar idempotencia.

### Bloque 4: conteos fisicos

- Capturar stock real en unidad base.
- Calcular diferencia y direccion.
- Exigir motivo.
- Rechazar conteo sin cambio.
- Mantener transaccion y cotizacion obsoleta.

### Bloque 5: ventas y lecturas

- Retirar costo de checkout y detalles.
- Simplificar estadisticas.
- Actualizar historial global y por producto.
- Mantener cantidades medidas y centimetros.

### Bloque 5A: retiro transversal del modelo de costo

- Retirar `inventory_cost_cents`, `average_cost_cents`, estados y estadisticas de costo de modelos Rust y TypeScript del runtime objetivo.
- Retirar `movement_cost_cents` de detalles, recibos, historial y UI de ventas.
- Retirar helpers de retiro proporcional, distribucion de costo y replay sin consumidores.
- Retirar variantes de valoracion y comandos manuales genericos.
- Simplificar respuestas de correccion para que no informen ventas afectadas ni valor de inventario.
- Actualizar README, QA y release activos para el modelo sin valoracion.
- Crear `scripts/validate-retired-inventory-costs.mjs` con una allowlist explicita por simbolo y ruta.
- Permitir nombres antiguos solamente en migraciones inmutables 001-012, preflight y proyecciones de origen v8/v12, backup/restore, pruebas explicitas de eliminacion y planes historicos marcados como reemplazados.
- Agregar `test:retired-costs` a `pnpm test`; cualquier coincidencia fuera de la allowlist falla localmente y en CI.

### Bloque 6: correccion de compra

- Reemplazar replay por actualizacion local de precio e importe.
- Mantener cotizacion, fingerprint, transaccion e idempotencia.
- Limitar la accion a compras v3.

### Bloque 7: interfaz e intents

- Separar formularios de compra y conteo.
- Agregar acciones con producto preseleccionado.
- Retirar valoracion y controles de costo.
- Versionar y recuperar intents sin confirmar formatos antiguos.
- Adaptar escritorio, movil, teclado y foco.

### Bloque 8: QA y release

- Actualizar fixtures v8 y v12.
- Incluir fixture con precio kilo capturado como precio gramo.
- Confirmar que ningun costo antiguo llega a SQLite 13.
- Probar backup y rollback a SQLite 12.
- Actualizar runbooks y candidato exacto.

## Pruebas requeridas

### Compras

- Precio obligatorio y positivo.
- Proveedor obligatorio y no vacio.
- Factura obligatoria y no vacia.
- Nota opcional.
- Compra fija calcula cantidad e importe.
- Compra medida por kg, L y m calcula importe half-up.
- Compra directa usa precio por unidad base.
- Importe total redondeado a cero se rechaza en cotizacion, creacion y correccion.
- Stock aumenta exactamente una vez.
- Reintento no duplica compra.
- Cambio de stock o conversion invalida cotizacion.
- Overflow revierte toda la operacion.

### Conteos

- Conteo mayor crea ajuste de entrada.
- Conteo menor crea ajuste de salida.
- Conteo cero puede agotar stock.
- Conteo igual se rechaza sin escritura.
- `adjustment_reason` obligatorio, persistido en `note` y `note_search_key`.
- Precio, proveedor, factura y presentacion se rechazan.
- Stock concurrente exige recotizar.
- Piezas, gramos, mililitros y centimetros funcionan.
- Reintento no duplica ajuste.

### Ventas

- Checkout descuenta stock sin consultar costos.
- Varias lineas del mismo producto validan demanda agregada.
- Precio personalizado y descuento se conservan.
- Detalles y movimientos no contienen costo de venta.
- Venta en centimetros conserva snapshots.
- Fallo intermedio revierte venta y stock.

### Correccion de compra

- Solo compra manual v3 es elegible.
- Cotizacion no escribe.
- Precio nuevo recalcula importe con conversion original.
- Cantidad, stock y metadatos nunca cambian.
- No se modifican ventas ni movimientos posteriores.
- Fingerprint obsoleto exige recotizar.
- Reintento aplicado es idempotente.
- Compras historicas sin precio no son elegibles.

### Migracion

- Base nueva termina en version 13.
- Fixture v8 migra a 13.
- Fixture v12 migra a 13.
- Base 13 objetivo reabre sin migrar.
- Versiones 9 a 11 se rechazan.
- Costos historicos conocidos y `NULL` se eliminan.
- Movimientos historicos de valoracion se eliminan y su secuencia no retrocede, incluso si contenian el ID mas alto.
- El caso kilo capturado como gramo no aparece en el esquema objetivo.
- Campos nuevos de compra quedan `NULL` para historia migrada.
- Stock, cantidades, ventas, pagos, relaciones e IDs se preservan.
- Secuencias se preservan.
- Centimetros son validos en productos, detalles y movimientos.
- Unidades desconocidas se rechazan.
- Backup SQLite 12 conserva el archivo original completo.
- Rollback restaura literalmente la version de origen, v8 o v12.
- Constraints SQL aceptan solamente la matriz v1/v2/v3 documentada.

### Interfaz

- No existe selector generico de operacion.
- Compra muestra precio, proveedor y factura obligatorios.
- Conteo no muestra datos comerciales.
- No existe valoracion manual.
- No aparecen costo promedio, valor a costo ni costo desconocido.
- Compra y conteo pueden iniciarse desde el producto.
- Historial distingue compra, ajuste y venta.
- Vista movil no depende de una tabla ancha para acciones.
- Teclado, foco, errores y estados pendientes son accesibles.

## Verificacion

Ejecutar como minimo:

```bash
pnpm test
pnpm test:retired-costs
pnpm build
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
cargo test --release --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
pnpm qa:migration-v8-v13
```

Tambien se debe ejecutar QA sobre:

- Fixture v8.
- Fixture v12 con costos correctos, incorrectos y desconocidos.
- Fixture v12 con ventas, ajustes, presentaciones y huecos de secuencia.
- Copia cerrada y anonimizada de produccion.
- Restauracion completa a la version de origen: v8 para fixture v8 y v12 para fixture/produccion v12.

## Criterios de aceptacion

- Registrar compra y ajustar existencias son flujos separados.
- Una compra exige precio, proveedor y factura.
- Un ajuste usa stock contado en unidad base y motivo obligatorio.
- Ningun ajuste solicita o persiste datos comerciales.
- No existe costo acumulado ni valoracion en esquema, backend o interfaz.
- Checkout no calcula costo de venta.
- Los precios historicos de SQLite 12 no se migran.
- Las compras nuevas conservan precio e importe propios.
- Corregir una compra no afecta ningun otro movimiento.
- Stock, ventas cobradas, cantidades y relaciones se preservan al migrar.
- El backup permite recuperar literalmente la version de origen; en produccion, SQLite 12.
- Centimetros y presentaciones medidas siguen funcionando extremo a extremo.
- Suites frontend, backend, migracion y release terminan correctamente.

## Riesgos y mitigaciones

### Perdida intencional de datos financieros

La eliminacion es una decision de producto confirmada porque los costos de origen no son confiables. Se mitiga mediante:

- Backup obligatorio antes de migrar.
- Manifiesto con ruta, version de origen y validacion completada del backup.
- Rollback ensayado.
- Aprobacion operativa previa al despliegue.

### Confusion entre precio de compra y precio al cliente

Los nombres persistidos y visibles incluiran `purchase`. La interfaz mostrara explicitamente la base del precio y el importe total antes de confirmar.

### Conteo capturado en unidad incorrecta

El conteo muestra una sola unidad base, sin presentaciones. La confirmacion muestra stock anterior, contado y diferencia.

### Intents antiguos

Nunca se reinterpretan automaticamente. Se consultan por token para detectar operaciones ya aplicadas y, si no existen, se exige nueva captura.

### Migracion 013 local incompatible

La forma anterior de SQLite 13 no se soporta porque nunca llego a produccion. Los entornos de desarrollo deben eliminar y recrear sus bases.

## Despliegue

La version local objetivo es `v0.2.1`. Permanecen pendientes la copia cerrada real, los gates del candidato, el rollback operativo, el tag y la publicacion.

1. Reemplazar migracion 013 y validadores antes de abrir una copia de produccion.
2. Ejecutar suites y QA sintetico v8/v12.
3. Obtener copia cerrada y anonimizada de produccion.
4. Confirmar `PRAGMA user_version = 12`.
5. Tomar snapshot de todos los datos no financieros que deben preservarse.
6. Identificar los campos financieros que se eliminaran y obtener aprobacion operativa.
7. Ejecutar preflight, backup y migracion sobre la copia.
8. Confirmar ausencia de columnas y valores financieros antiguos.
9. Confirmar igualdad literal de stock, ventas, cantidades, relaciones y secuencias.
10. Probar compra, conteo, correccion de compra, POS y centimetros.
11. Restaurar el backup SQLite 12 y comparar literalmente el archivo logico restaurado.
12. Validar el candidato exacto en escritorio y movil.
13. Cerrar produccion antes de instalar.
14. Conservar el backup SQLite 12 durante el periodo operativo acordado.

La primera prueba sobre datos reales se realiza siempre en una copia desechable, nunca sobre el archivo activo de produccion.
