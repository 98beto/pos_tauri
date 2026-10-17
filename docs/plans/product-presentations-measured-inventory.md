# Presentaciones de producto e inventario medido

**Estado:** implementado.

**Evolucion:** el modelo de costo de compra, costo promedio y valoracion sigue vigente. [`measured-presentations-inventory-cost-corrections.md`](./measured-presentations-inventory-cost-corrections.md) reemplaza la captura de stock y costo inicial, agrega centimetros y define la correccion segura de precios de compra.

**Referencia visual:** `demo.html` contiene el prototipo navegable acordado para Productos, POS, Inventario y Ventas. Sus datos son simulados y no representan funcionalidad ya implementada.

## Objetivo

Permitir que un mismo producto se venda en distintas presentaciones mientras todas descuentan de un unico inventario expresado en una unidad base.

Casos principales:

- Vender un tornillo por pieza a `$1` o en una bolsa de 100 a `$80`.
- Vender productos por peso, capturando gramos y mostrando kilogramos.
- Vender liquidos por volumen, capturando mililitros y mostrando litros.
- Registrar compras y ajustes con costo, proveedor, documento y nota por movimiento.
- Valorar el inventario con costo promedio ponderado.
- Consultar una subvista de inventario por producto con saldos, costos y documentos.
- Actualizar el nombre visible de un producto en las vistas operativas sin alterar tickets historicos.
- Actualizar bases existentes sin perder productos, stock, ventas ni movimientos.

## Rama de trabajo

Crear la rama desde el estado actual de `feat/v0.1.1-updater-fixes`, como se acordo:

```bash
git switch feat/v0.1.1-updater-fixes
git switch -c feat/product-presentations-inventory
```

No mezclar la implementacion directamente en `main` ni modificar migraciones ya publicadas.

## Decisiones de dominio

### Producto y SKU

- El SKU obligatorio y unico pertenece al producto, no a cada presentacion.
- Una presentacion no requiere que el negocio invente otro SKU.
- Una presentacion puede tener un `barcode` opcional cuando exista un codigo real diferente en el empaque.
- El carrito, las ventas y los movimientos identifican la presentacion mediante `presentation_id`, no mediante SKU.
- Buscar el SKU del producto muestra el producto y sus presentaciones activas.
- Escanear un `barcode` de presentacion puede seleccionar esa presentacion directamente.
- La validacion del servicio debe impedir que un codigo opcional produzca una busqueda ambigua con otro producto o presentacion.

Ejemplo:

```text
Producto: Tornillo
SKU: TOR-001
Unidad base: pieza

Presentaciones:
- Pieza: fija, factor 1, precio $1
- Bolsa 100: fija, factor 100, precio $80
```

### Unidades base

Cada producto usa una sola unidad base:

- `piece`: pieza entera.
- `gram`: gramo entero.
- `milliliter`: mililitro entero.

La unidad base es inmutable despues de crear el producto. Esto evita reinterpretar stock, ventas y movimientos existentes.

Las cantidades y el dinero se guardan como enteros. No se usa `REAL`, `f32` ni `f64` para reglas de negocio.

### Presentaciones

Cada presentacion contiene:

- Producto relacionado.
- Nombre, por ejemplo `Pieza`, `Bolsa 100`, `Kilogramo` o `Litro`.
- Modo `fixed` o `measured`.
- Cantidad base o base de precio.
- Precio en centavos.
- Codigo de barras opcional.
- Estado activo.
- Indicador de presentacion predeterminada.

Semantica:

- Una presentacion `fixed` consume `cantidad vendida * base_quantity`.
- Una presentacion `measured` recibe directamente gramos o mililitros y usa `base_quantity` como base de precio.
- Una presentacion referenciada por ventas o movimientos no se elimina; se desactiva.
- El modo y el factor de una presentacion usada no se modifican. Para cambiar su conversion se desactiva y se crea otra.
- El nombre, precio y codigo de barras pueden cambiar; ventas y movimientos conservan sus snapshots.

Ejemplos:

```text
Pieza:
mode = fixed
base_quantity = 1

Bolsa de 100:
mode = fixed
base_quantity = 100

Kilogramo:
mode = measured
base_quantity = 1000 gramos

Litro:
mode = measured
base_quantity = 1000 mililitros
```

### Nombres actuales e historicos

`products.name` es la fuente del nombre actual.

- Productos, POS, carrito, inventario global y detalle de inventario muestran el nombre actual mediante un `JOIN` por `product_id`.
- Cambiar el nombre del producto se refleja inmediatamente en esas vistas.
- No se ejecutan actualizaciones masivas para reescribir nombres duplicados.
- Los snapshots existentes en ventas y movimientos permanecen sin cambios para auditoria.
- Tickets y ventas pasadas muestran el nombre registrado al momento de vender.
- Cuando sea util para auditoria, el detalle puede mostrar tanto el nombre actual como el snapshot historico.
- La busqueda operativa usa el nombre y SKU actuales; una busqueda historica puede incluir tambien snapshots, proveedor, documento y ticket.

## Aritmetica y redondeo

Centralizar en Rust la conversion de cantidades, el calculo de lineas y el costo promedio.

Usar `i128` para operaciones intermedias y comprobar que todo valor persistido o enviado por IPC permanezca dentro de `JS_MAX_SAFE_INTEGER`.

Presentacion fija:

```text
base_consumed = entered_quantity * base_quantity
line_total = entered_quantity * price_cents
```

Presentacion medida:

```text
base_consumed = entered_quantity
line_total = round_half_up(entered_quantity * price_cents / base_quantity)
```

Reglas:

- `entered_quantity` siempre es un entero positivo.
- Para peso se captura en gramos; para volumen, en mililitros.
- Una linea cuyo total redondee a cero se rechaza.
- El total persistido de una linea es definitivo y nunca se reconstruye usando precios actuales.
- La suma comprobada de las lineas debe ser igual al total de la venta.

## Costo promedio ponderado

Guardar en el producto el costo total acumulado del stock, no un promedio redondeado por unidad.

Entrada con costo conocido:

```text
stock_after = stock_before + incoming_base_quantity
cost_after = cost_before + incoming_total_cost
```

Salida con costo conocido:

```text
removed_cost = round_half_up(cost_before * outgoing_base_quantity / stock_before)
stock_after = stock_before - outgoing_base_quantity
cost_after = cost_before - removed_cost
```

Si la salida consume todo el stock, el costo retirado es el costo completo y el nuevo costo acumulado es cero.

Reglas:

- Una compra requiere costo total de adquisicion.
- Un ajuste de entrada requiere costo total para no crear inventario sin valor.
- Una venta o ajuste de salida calcula automaticamente el costo retirado.
- El costo promedio se usa internamente para mantener la valoracion del stock y calcular salidas; la interfaz principal muestra el valor total conocido sin destacar el promedio como columna operativa.
- El valor de inventario es la suma de los costos acumulados conocidos, no `precio de venta * stock`.

### Stock legado sin costo

No inferir el costo de productos existentes usando el precio de venta.

- Un producto migrado con stock cero recibe costo acumulado cero.
- Un producto migrado con stock positivo recibe costo desconocido.
- La aplicacion puede continuar vendiendo y ajustando ese stock.
- Mientras el costo sea desconocido, no se publica un promedio ni un valor contable falso.
- Las entradas conservan su costo individual aunque el costo acumulado siga siendo desconocido.
- Si el stock desconocido llega a cero, el costo acumulado se normaliza a cero.
- Agregar una accion auditada `Establecer costo actual` para asignar el costo total vigente sin cambiar el stock.
- La valoracion genera un movimiento con cantidad cero, tipo `valoracion` y costo anterior/posterior.

## Migraciones

No modificar `001` a `008`. Agregar migraciones secuenciales y registrarlas en `src-tauri/src/db/migrations.rs`.

### 009: productos y presentaciones

Agregar a `products`:

- `base_unit TEXT NOT NULL DEFAULT 'piece'` con dominio cerrado.
- `inventory_cost_cents INTEGER NULL` con limites de entero seguro.

Mantener:

- `products.sku` y `products.sku_search_key` como SKU canonico y unico del producto.
- `products.price` temporalmente como espejo del precio de la presentacion predeterminada para reducir el riesgo de compatibilidad. El codigo nuevo no lo usa como fuente autoritativa.

Crear `product_presentations` con:

- `id`.
- `product_id`.
- `name`.
- `mode` con valores `fixed` o `measured`.
- `base_quantity` entero positivo.
- `price` entero positivo en centavos.
- `barcode` y `barcode_search_key` opcionales.
- `is_default` e `is_active` como enteros booleanos.
- Fechas de creacion y actualizacion.

Indices y restricciones:

- Codigo de barras unico cuando no sea `NULL`.
- Como maximo una presentacion predeterminada por producto mediante indice parcial.
- Indice por producto y estado activo.
- El servicio garantiza que siempre exista exactamente una predeterminada.

Backfill:

- Cada producto existente conserva nombre, SKU, precio y stock.
- Cada producto existente obtiene `base_unit = piece`.
- Crear una presentacion `Pieza`, fija, con factor `1`, SKU heredado solo a nivel de producto y precio actual.
- Marcarla activa y predeterminada.
- Asignar costo cero solamente cuando el stock sea cero.

### 010: detalles de venta

Reconstruir `sale_details` porque SQLite no permite retirar directamente `UNIQUE(sale_id, product_id)`.

Agregar:

- `record_version`.
- `presentation_id`.
- Nombre y codigo de barras de presentacion como snapshots.
- Nombre y SKU de producto como snapshots.
- Unidad base y modo como snapshots.
- Factor/base de precio como snapshot.
- Cantidad ingresada.
- Cantidad base consumida.
- Total final de linea.
- Costo retirado de inventario, nullable para registros legados.

Reemplazar la unicidad por producto con `UNIQUE(sale_id, presentation_id)` para los registros nuevos. El carrito consolida cada presentacion en una sola linea, pero una venta puede contener varias presentaciones del mismo producto.

Backfill:

- Preservar IDs, venta, producto, cantidad, precio, fechas y snapshots.
- Asociar cada linea antigua con la presentacion predeterminada creada en `009`.
- Interpretar su cantidad como piezas y conservar el total historico de la venta como autoridad.
- Marcar los registros como version legada.
- Recrear todos los indices aplicables de `008`.

### 011: movimientos de inventario

Reconstruir `inventory_movements` para retirar `UNIQUE(sale_id, product_id)` y soportar un movimiento por linea/presentacion.

Agregar:

- `record_version`.
- `presentation_id` opcional.
- `sale_detail_id` opcional y unico cuando exista.
- Snapshots de producto y presentacion.
- Cantidad ingresada.
- Cantidad base aplicada.
- Stock anterior y posterior.
- Costo del movimiento.
- Costo acumulado anterior y posterior.
- Proveedor opcional.
- Referencia de factura/documento opcional.
- Nota opcional.
- Datos actuales de idempotencia.

Reglas de coherencia:

- Venta: origen `sale`, motivo `venta`, tipo `salida`, con venta y detalle relacionados.
- Stock inicial: origen `initial_stock`, motivo `ajuste`, tipo `entrada`.
- Compra manual: origen `manual`, motivo `compra`, tipo `entrada`, costo positivo y token.
- Ajuste de entrada: origen `manual`, motivo `ajuste`, tipo `entrada`, costo positivo y token.
- Ajuste de salida: origen `manual`, motivo `ajuste`, tipo `salida` y token.
- Valoracion: origen `manual`, motivo y tipo `valoracion`, cantidad base cero y token.
- Los movimientos nuevos guardan stock anterior/posterior.
- Los movimientos legados pueden tener saldos y costos nulos.

Backfill:

- Preservar todos los IDs, snapshots, tokens, cantidades, ventas y fechas.
- Marcar los registros como version legada.
- Asociar la presentacion predeterminada cuando sea posible.
- Copiar la cantidad existente como cantidad base por tratarse de piezas.
- No inventar saldos anteriores/posteriores ni costos historicos.
- No recalcular ni sobrescribir `products.stock`.

Indices nuevos:

- Producto y movimiento descendente para el detalle.
- `sale_detail_id`.
- Proveedor y referencia documental si las busquedas lo requieren.
- Conservar indices por venta, origen, motivo, tipo, token y claves de busqueda.

## Backend Rust

### Modelos

Modificar:

- `src-tauri/src/models/product.rs`.
- `src-tauri/src/models/cart.rs`.
- `src-tauri/src/models/sale.rs`.
- `src-tauri/src/models/sale_detail.rs`.
- `src-tauri/src/models/inventory_movement.rs`.
- `src-tauri/src/models/mod.rs`.

Agregar `src-tauri/src/models/product_presentation.rs`.

Los DTO deben exponer:

- Unidad base y stock base del producto.
- Estado de valoracion, costo acumulado y promedio mostrado.
- Presentaciones activas y predeterminada.
- Cantidad ingresada y cantidad base en carrito, ventas y movimientos.
- Totales de linea calculados por Rust.
- Metadatos de proveedor, documento y nota.
- Detalle de inventario con producto actual, resumen y movimientos.

No sumar piezas, gramos y mililitros en un unico `item_count` o `inventory_units`. Exponer cantidad de lineas y cantidades agrupadas por unidad cuando se necesiten.

### Servicios compartidos

En `src-tauri/src/services/mod.rs`:

- Centralizar el parseo estricto de dinero textual.
- Agregar multiplicacion comprobada.
- Agregar division positiva con redondeo half-up.
- Agregar conversion de presentacion a cantidad base.
- Agregar calculo de total de linea.
- Agregar calculo y retiro proporcional del costo acumulado.

### Productos y presentaciones

Modificar `src-tauri/src/services/products.rs` y los comandos de productos.

Agregar un servicio de presentaciones o mantenerlo en productos si no crece lo suficiente para justificar otro modulo.

- Crear producto, presentaciones, stock, costo inicial y movimiento inicial en una transaccion `IMMEDIATE`.
- Actualizar nombre, SKU, marca y categoria sin tocar snapshots historicos.
- Mantener `products.price` sincronizado con la presentacion predeterminada mientras exista la columna legada.
- Validar una presentacion predeterminada y al menos una presentacion activa.
- Buscar por nombre actual, SKU del producto y codigo de barras opcional.
- No eliminar presentaciones referenciadas.
- Impedir cambios de unidad base.

### Carrito

Modificar `src-tauri/src/commands/mod.rs`, `services/cart.rs` y `commands/cart.rs`.

- Cambiar el carrito de `BTreeMap<product_id, quantity>` a `BTreeMap<presentation_id, entered_quantity>`.
- Resolver producto y presentacion al construir cada linea.
- Calcular cantidad base y subtotal en Rust.
- Agrupar la demanda por producto antes de decidir disponibilidad.
- Permitir pieza y bolsa del mismo producto simultaneamente.
- Crear un fingerprint nuevo que incluya presentacion, cantidad, factor, precio y total redondeado.
- Mantener compatibilidad de lectura con ventas e intents antiguos; no reinterpretarlos como el formato nuevo.

### Checkout y ventas

Modificar `src-tauri/src/services/sales.rs`.

- Usar una transaccion `IMMEDIATE`.
- Cargar y validar todas las presentaciones.
- Calcular y persistir cada total de linea.
- Agrupar la cantidad base por producto.
- Ejecutar una sola comprobacion y reduccion de stock por producto.
- Retirar costo una sola vez por producto.
- Distribuir el costo retirado entre sus lineas de forma determinista, asignando cualquier centavo residual por orden estable.
- Insertar venta, detalles, movimientos y actualizaciones de producto en la misma transaccion.
- Relacionar cada movimiento de venta con su `sale_detail_id`.
- Reconstruir retries por `presentation_id`, no por `product_id`.
- Leer los totales historicos persistidos; nunca recalcularlos con presentaciones actuales.

### Inventario

Modificar `src-tauri/src/services/inventory.rs` y `commands/inventory.rs`.

- Conservar el comportamiento de ajustes como deltas, no como asignacion absoluta de stock.
- Mantener token, fingerprint, retry idempotente y transaccion `IMMEDIATE`.
- Incluir presentacion, cantidad, costo, proveedor, documento y nota en el fingerprint nuevo.
- Una compra o ajuste de entrada incrementa stock y costo.
- Una salida reduce stock y costo promedio.
- Una valoracion cambia solo el costo acumulado.
- Agregar `get_product_inventory_detail` autenticado.
- Las consultas de inventario unen `products` para presentar nombre y SKU actuales.
- Los snapshots quedan disponibles para auditoria y busqueda historica.

Registrar los comandos nuevos en `src-tauri/src/lib.rs`.

## Frontend

### Tipos y adaptadores

Modificar:

- `src/types/product.ts`.
- `src/types/cart.ts`.
- `src/types/sale.ts`.
- `src/types/sale_detail.ts`.
- `src/types/inventory_movement.ts`.
- `src/api/products.ts`.
- `src/api/cart.ts`.
- `src/api/inventory.ts`.

Agregar ayudantes de formato visual en `src/lib/format.ts`:

- Piezas como enteros.
- Gramos mostrados en kilogramos con hasta tres decimales.
- Mililitros mostrados en litros con hasta tres decimales.
- Etiquetas de precio por pieza, kilogramo o litro.

Los ayudantes solo formatean; no calculan stock, costo ni totales de negocio.

### Productos

Actualizar `src/features/products/ProductsView.tsx`.

- El formulario del producto conserva un unico SKU obligatorio.
- Agregar selector de unidad base.
- Agregar editor de presentaciones con nombre, forma de venta, cantidad contenida o base de precio, precio y codigo de barras opcional.
- Evitar los terminos tecnicos `Fija`, `Medida`, `Modo` y `Factor` en la interfaz. Mostrar `Como se vende`, `Cantidad definida`, `Cantidad capturada al vender`, `Cantidad que contiene` y `Precio calculado por`.
- Para existencia inicial, solicitar `Precio de compra por unidad base` y mostrar el `Valor inicial del inventario` calculado. No presentar un campo ambiguo llamado `Costo inicial`.
- No solicitar SKU por presentacion.
- Mostrar stock formateado segun unidad base.
- Mostrar presentacion predeterminada y numero de presentaciones activas.
- Al editar, la unidad base permanece de solo lectura.
- Administrar activacion y presentacion predeterminada sin eliminar historia.

### POS

Actualizar `src/features/pos/PosView.tsx`.

- Buscar productos por nombre, SKU o codigo de barras.
- Mostrar las presentaciones activas del producto.
- Una presentacion fija usa cantidad de piezas/paquetes.
- Una presentacion medida abre captura en gramos o mililitros con `inputMode="numeric"`.
- Las lineas se identifican por presentacion.
- Mostrar precio por presentacion, kilogramo o litro.
- Mantener subtotales y total recibidos desde Rust.
- Mostrar presentacion y cantidad correctamente en checkout y recibo.
- No usar botones `+/-` para incrementar un gramo o mililitro; ofrecer `Editar cantidad`.

### Registro manual de inventario

Actualizar `src/features/inventory/manual-movement-intent.ts` a una nueva version compatible con intents pendientes anteriores.

Actualizar el formulario de `InventoryView`:

- Producto.
- Presentacion o captura directa en unidad base.
- Operacion.
- Cantidad.
- Precio unitario aplicado para compra o entrada.
- Importe total calculado y confirmado por Rust.
- Proveedor opcional.
- Factura/referencia opcional.
- Nota opcional.

Los datos de proveedor y documento pertenecen a cada movimiento, como se acordo; no se crea catalogo de proveedores en este alcance.

Persistir el intent antes del IPC y conservar las reglas actuales de retry, compare-and-clear y aislamiento por usuario.

### Vista principal de inventario

Separar la vista principal en dos pestañas para que un catalogo grande no empuje el historial fuera de alcance:

- `Productos`: busqueda por nombre/SKU, filtros por unidad y estado, tabla con `Ver inventario` y paginacion de 20 registros.
- `Movimientos`: busqueda y filtros propios, historial global y paginacion de 20 registros.
- No mostrar ambas tablas largas simultaneamente.
- Conservar pestaña, pagina, busqueda y filtros al abrir un producto y regresar.
- La consulta backend debe paginar; el frontend no debe cargar todo el catalogo para recortarlo localmente.

### Detalle de inventario

Crear `src/features/inventory/ProductInventoryDetail.tsx`.

`InventoryView` conserva localmente `selectedProductId` y alterna entre lista y detalle. No agregar React Router ni otro elemento al menu lateral.

Acceso:

- Boton explicito `Ver inventario` en las filas.
- Selector/buscador de producto para acceder tambien a productos sin movimientos.
- Boton visible `Volver al inventario`.
- Al volver, conservar filtros y restaurar foco cuando sea posible.

Cabecera:

- Nombre y SKU actuales.
- Unidad base.
- Stock actual.
- Valor a costo.
- Estado `Sin costo registrado` cuando no pueda calcularse.
- Ultimo movimiento.

Filtros:

- Motivo.
- Tipo.
- Fecha inicial y final.
- Busqueda por proveedor, documento, nota o ticket.

Tabla:

- Fecha.
- Tipo y motivo.
- Presentacion.
- Cantidad ingresada y cantidad base cuando aporten contexto.
- Precio unitario aplicado: precio de compra en entradas, precio de venta en ventas y costo de inventario en ajustes de salida.
- Importe total del movimiento.
- Proveedor/documento.
- Ticket o nota.

El backend conserva `stock_before` y `stock_after` para integridad y auditoria, pero la tabla principal no muestra el saldo restante por movimiento.

En movil, mantener filtros en una columna, acciones tactiles visibles y tabla con desplazamiento horizontal.

### Estadisticas

- Sustituir el valor actual `precio de venta * stock` por `Valor total a costo`.
- Mostrar `Sin costo registrado` como conteo separado y aclarar que esos productos no estan incluidos en el total.
- Agrupar existencias por piezas, gramos y mililitros en lugar de sumarlas.
- No presentar gramos o mililitros como cantidad de articulos vendidos.

## Pruebas de migracion

Extender `src-tauri/src/db/migrations.rs` con una prueba que construya una base en version 8 y contenga:

- Producto con stock positivo.
- Producto sin stock.
- Venta y detalle historicos.
- Movimiento de venta.
- Movimiento manual.
- Movimiento de stock inicial.
- Nombre y SKU historicos distintos de los actuales.
- Tokens y fechas conocidos.

Despues de migrar:

- La version debe ser la ultima esperada.
- Cada producto debe tener una presentacion predeterminada.
- El stock debe permanecer identico.
- IDs, ventas, totales, cantidades, timestamps, snapshots y tokens deben permanecer identicos.
- El stock positivo legado debe tener costo desconocido.
- Los registros viejos deben estar marcados como legados.
- Las claves foraneas deben ser validas.
- Debe ser posible vender dos presentaciones del mismo producto.
- Un fallo durante copia/reconstruccion debe revertir toda la migracion y conservar la version anterior.

Agregar tambien prueba con un archivo SQLite real cerrado y reabierto despues de migrar.

## Pruebas Rust

Productos y presentaciones:

- Backfill y creacion de presentacion predeterminada.
- SKU unico a nivel de producto.
- Presentacion sin SKU propio.
- Codigo de barras opcional unico y sin ambiguedades.
- Unidad base inmutable.
- Creacion atomica con stock y costo inicial.
- Desactivacion sin perdida de historia.

Carrito y ventas:

- Pieza y bolsa del mismo producto en una venta.
- Demanda agregada impide vender mas stock usando varias presentaciones.
- Peso y volumen aplican redondeo half-up.
- Una linea que redondea a cero se rechaza.
- Totales persistidos coinciden con la venta.
- Retry no descuenta stock ni costo dos veces.
- Fallos intermedios revierten venta, detalles, movimientos, stock y costo.
- Recibos legados siguen siendo legibles.

Costo e inventario:

- Compra actualiza stock y costo acumulado.
- Ajuste de entrada requiere costo.
- Salida retira costo promedio.
- Agotamiento deja stock y costo en cero.
- Distribucion de centavos no crea ni pierde costo.
- Stock legado permanece con valoracion desconocida.
- Valoracion manual no cambia el stock.
- Proveedor, documento y nota sobreviven cambios del producto y reinicio.
- Fingerprint incluye todos los campos nuevos.
- Dos conexiones con el mismo token aplican el movimiento una sola vez.
- Detalle usa nombre actual y conserva snapshots historicos.

## Pruebas frontend

Actualizar fixtures y pruebas de adaptadores, productos, POS, ventas e inventario.

Cubrir:

- Producto con un SKU y varias presentaciones sin SKU propio.
- Codigo de barras opcional.
- Etiquetas de piezas, gramos, kilogramos, mililitros y litros.
- Cantidades medidas vacias, cero, decimales, con signo o fuera de stock.
- Carrito con pieza y bolsa del mismo producto.
- Edicion de cantidades medidas.
- Captura y recuperacion de costo, proveedor, documento y nota.
- Compatibilidad de intents pendientes anteriores.
- Nombre actual en inventario despues de renombrar un producto.
- Nombre historico conservado en un ticket anterior.
- Detalle: carga inicial, vacio, filtros, error, retry y respuestas fuera de orden.
- Preservacion de filas anteriores durante recargas.
- Sesion expirada y desmontaje seguro.
- Navegacion de regreso y accesibilidad por teclado.

## Criterios de aceptacion

### Tornillos

```text
Stock inicial: 250 piezas
Pieza: $1, factor 1
Bolsa 100: $80, factor 100

Venta: 1 bolsa + 3 piezas
Total: $83
Consumo: 103 piezas
Stock final: 147 piezas
Lineas de venta: 2
```

### Compra de tornillos

```text
Producto: Tornillo, SKU TOR-001
Presentacion: Bolsa 100
Cantidad recibida: 10
Entrada base: 1,000 piezas
Proveedor: opcional
Factura/referencia: opcional
Precio unitario de compra: obligatorio
Importe total: calculado como 10 × precio unitario
```

No se crea un SKU artificial para la bolsa.

### Peso

```text
Unidad base: gramo
Presentacion: Kilogramo
Precio: $120 por kg
Cantidad vendida: 250 g
Consumo: 250 g
Total: $30
```

### Cambio de nombre

```text
Nombre anterior: Tornillo estandar
Nombre actual: Tornillo galvanizado
```

- Productos, POS e inventario muestran `Tornillo galvanizado`.
- Un ticket emitido antes del cambio conserva `Tornillo estandar`.
- El detalle puede mostrar el snapshot anterior como informacion de auditoria.

## Orden de implementacion

1. Crear rama y pruebas de migracion desde version 8.
2. Implementar migraciones `009` a `011`.
3. Implementar aritmetica comprobada y modelos Rust.
4. Implementar productos y presentaciones.
5. Convertir carrito y selector a `presentation_id`.
6. Convertir checkout, ventas, stock y costo de forma atomica.
7. Extender movimientos manuales e idempotencia.
8. Implementar detalle de inventario y consultas con nombre actual.
9. Actualizar tipos, adaptadores y UI React.
10. Completar pruebas de regresion, build y comprobacion manual.

## Verificacion

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo check --release --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
cargo test --release --manifest-path src-tauri/Cargo.toml
pnpm test
pnpm build
```

## Comprobacion manual

1. Actualizar una copia de una base version 8 y comprobar que productos, stock, ventas y movimientos siguen visibles.
2. Crear un producto por pieza con presentaciones Pieza y Bolsa 100 usando un solo SKU.
3. Vender ambas presentaciones en una venta y confirmar total, stock, costo y recibo.
4. Crear productos por peso y volumen y confirmar conversiones y redondeo.
5. Registrar una compra con costo, proveedor y factura, reiniciar la aplicacion y verificar persistencia.
6. Registrar ajustes de entrada y salida y comprobar el costo promedio.
7. Valorar un producto legado sin cambiar su stock.
8. Renombrar un producto y confirmar el nombre actual en POS e inventario y el snapshot anterior en tickets.
9. Abrir el detalle desde inventario, aplicar filtros y volver sin perder los filtros globales.
10. Simular retries y errores para comprobar que ninguna operacion duplica stock, costo, venta o movimiento.
