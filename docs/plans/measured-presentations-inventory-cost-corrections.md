# Presentaciones medidas, centimetros y correccion de costos

**Estado:** implementado en el repositorio. La suite sintetica esta disponible; sigue pendiente la validacion operativa sobre una copia cerrada y anonimizada de produccion, seguida del smoke y rollback del candidato exacto.

**Esquema de origen:** SQLite 12.

**Esquema objetivo:** SQLite 13.

## Objetivo

Agregar longitud en centimetros, simplificar la configuracion de productos medidos y conservar el modelo de costos de compra existente.

El resultado esperado es:

- Los productos por peso, volumen y longitud usan presentaciones `measured`.
- Las bases estandar `1 kg`, `1 L` y `1 m` se eligen mediante una opcion clara y no permiten editar su conversion.
- Sigue siendo posible definir otra base de precio, por ejemplo `100 g`, `500 ml` o `25 cm`.
- Los centimetros son la unidad base persistida para productos de longitud.
- Los productos nuevos se crean con stock cero y costo acumulado cero.
- Crear un producto no solicita stock inicial ni precio de compra y no genera un movimiento inicial.
- Las compras y ajustes de entrada se registran desde Inventario con cantidad y precio de compra.
- Se conservan el costo acumulado, costo promedio, valor a costo y costo retirado por ventas o ajustes.
- Se puede corregir el precio de una compra existente sin modificar su cantidad.
- Corregir una compra recalcula transaccionalmente todos los costos posteriores que dependan de ella.
- La migracion 013 conserva literalmente la informacion financiera de SQLite 12 y solo amplia las unidades para admitir centimetros.
- La actualizacion crea y valida un backup antes de modificar una base instalada.

## Alcance

Este plan modifica:

- Modelo y validaciones de productos y presentaciones.
- Formulario de alta y edicion de productos.
- Unidad base, formateo y captura de longitud.
- Alta manual, cotizacion e historial de inventario.
- Correccion de precios de compras existentes.
- Replay cronologico de costos de inventario.
- Checkout y movimientos de inventario generados por ventas.
- Estadisticas de productos e inventario.
- Contratos Rust, TypeScript e IPC.
- Esquema SQLite, migracion 013, backup y recuperacion.
- Pruebas de frontend, backend y migracion.
- Documentacion de QA, release y rollback.

## Fuera de alcance

- Editar la cantidad de una compra historica.
- Editar la presentacion, conversion, producto, fecha o tipo de un movimiento historico.
- Borrar ventas, detalles o movimientos.
- Cambiar la unidad base de un producto existente.
- Reabrir o cambiar los totales comerciales cobrados en una venta.
- Agregar una tabla de auditoria para las correcciones de precio.
- Conservar el precio anterior despues de confirmar una correccion.
- Unidades base con decimales.
- Paquetes `fixed` para productos por peso, volumen o longitud.
- Calcular margen bruto como una nueva funcionalidad visible.
- Agregar importes de inventario calculados con precios al cliente.
- Valorar el inventario con precios de venta en vez de costos de compra.
- Distribuir descuentos comerciales sobre costos de inventario.

## Estado actual relevante

SQLite 12 maneja:

- `products.base_unit`: `piece`, `gram` o `milliliter`.
- `product_presentations.mode`: `fixed` o `measured`.
- `products.inventory_cost_cents`: costo acumulado del stock actual.
- `sale_details.movement_cost_cents`: costo retirado por una linea de venta.
- Snapshots de costo antes y despues en `inventory_movements`.
- Precio de compra e importe de costo por movimiento manual.

La semantica de presentaciones es:

```text
fixed:
base_consumed = entered_quantity * base_quantity
line_total = entered_quantity * price_cents

measured:
base_consumed = entered_quantity
line_total = round_half_up(entered_quantity * price_cents / base_quantity)
```

`Kilogramo` y `Litro` no son unidades persistidas. Son presentaciones medidas con bases de precio de 1000 gramos o 1000 mililitros. `Metro` seguira el mismo modelo con una base de 100 centimetros.

El costo acumulado representa el costo total del stock vigente. El costo promedio visible se deriva mediante:

```text
average_cost = round_half_up(inventory_cost_cents / stock)
```

El arbol de trabajo contenia una implementacion local anterior de SQLite 13 incompatible con este modelo. Esa direccion fue reemplazada antes de publicar o abrir una base de produccion.

## Decisiones confirmadas

### Version del esquema

No se reducira `CURRENT_SCHEMA_VERSION` a 12.

El objetivo seguira siendo SQLite 13 porque la estructura persistida admite una unidad adicional. Conceptualmente, el esquema objetivo sera:

```text
SQLite 13 = SQLite 12 + centimeter
```

La migracion 013 conserva todas las columnas de costo y no agrega columnas de importe comercial.

### Modos por unidad base

Las combinaciones admitidas para datos nuevos seran:

| Unidad base | Modo admitido | Uso |
| --- | --- | --- |
| `piece` | `fixed` | Pieza o paquete de piezas |
| `gram` | `measured` | Peso capturado en gramos |
| `milliliter` | `measured` | Volumen capturado en mililitros |
| `centimeter` | `measured` | Longitud capturada en centimetros |

`PresentationMode` conserva las variantes `fixed | measured`.

Rust valida la combinacion unidad/modo. La interfaz evita crear combinaciones invalidas, pero no es la autoridad de dominio.

La migracion no convierte automaticamente presentaciones historicas ni cambia el significado de `entered_quantity`.

### Selector de base de precio

Para una presentacion medida, la interfaz mostrara:

| Unidad | Opcion estandar | Base bloqueada | Opcion editable |
| --- | --- | --- | --- |
| Gramo | `Kilogramo` | `1000 g` | `Otra medida` |
| Mililitro | `Litro` | `1000 ml` | `Otra medida` |
| Centimetro | `Metro` | `100 cm` | `Otra medida` |

La opcion se deriva de `base_unit` y `base_quantity`; no requiere una columna nueva.

```text
gram + 1000         -> Kilogramo
milliliter + 1000   -> Litro
centimeter + 100    -> Metro
cualquier otra base -> Otra medida
```

Al elegir la opcion estandar:

- La cantidad muestra la base canonica.
- El campo queda deshabilitado.
- El payload envia siempre la base canonica.

Al elegir `Otra medida`:

- La base debe ser un entero positivo.
- Debe ser segura para JavaScript.
- Puede coincidir posteriormente con una base estandar; al editar se mostrara como estandar.

El nombre comercial sigue siendo editable. Las reglas no dependen de textos como `Kilogramo`, `Litro` o `Metro`.

### Longitud

La nueva unidad persistida es `centimeter`.

- El stock se guarda como centimetros enteros.
- Productos, Inventario y POS capturan centimetros enteros.
- El precio por metro usa `mode = measured` y `base_quantity = 100`.
- El formato visible puede convertir centimetros a metros sin usar punto flotante para reglas de negocio.
- La unidad base permanece inmutable despues de crear el producto.

Ejemplo comercial:

```text
Producto: Cable
base_unit = centimeter
stock = 850

Presentacion: Metro
mode = measured
base_quantity = 100
price = 2500 centavos

Venta de 75 cm:
base_consumed = 75
line_total = round_half_up(75 * 2500 / 100)
           = 1875 centavos
```

### Creacion de productos

Productos deja de capturar stock inicial y precio de compra inicial.

Todo producto nuevo se crea con:

```text
stock = 0
inventory_cost_cents = 0
```

No se crea un movimiento `initial_stock`.

Usar costo acumulado cero no significa que el usuario haya declarado un precio de compra. Es el saldo correcto de un inventario vacio y permite que la primera compra establezca un costo conocido.

Los contratos de creacion dejan de contener:

- `initial_stock`.
- `initial_purchase_price_per_base`.

No se mantendra compatibilidad IPC para esos campos.

### Modelo de costos

Se conserva la semantica de SQLite 12.

Para una entrada con costo conocido:

```text
stock_after = stock_before + base_quantity
movement_cost = costo de la cantidad capturada
inventory_cost_after = inventory_cost_before + movement_cost
```

Para una salida con costo conocido:

```text
movement_cost = round_half_up(
  inventory_cost_before * base_quantity / stock_before
)
inventory_cost_after = inventory_cost_before - movement_cost
```

Al retirar todo el stock conocido:

```text
stock_after = 0
inventory_cost_after = 0
```

Si el costo acumulado es desconocido:

- Se conserva `NULL` mientras quede stock.
- Una compra puede conservar su costo individual, aunque el total siga siendo desconocido.
- Agotar el stock normaliza el costo acumulado a cero.
- Una valoracion establece un nuevo costo acumulado conocido.

### Compras y ajustes

Una compra o ajuste de entrada captura:

- Producto.
- Presentacion opcional.
- Cantidad.
- Precio de compra.
- Proveedor opcional.
- Documento opcional.
- Nota opcional.

El precio se interpreta segun la forma de captura seleccionada:

```text
fixed:
base_quantity = entered_quantity * presentation.base_quantity
movement_cost = entered_quantity * purchase_price

measured:
base_quantity = entered_quantity
movement_cost = round_half_up(
  entered_quantity * purchase_price / presentation.base_quantity
)

captura directa sin presentacion:
base_quantity = entered_quantity
movement_cost = entered_quantity * purchase_price_per_base
```

Un ajuste de salida no captura precio. Retira costo proporcional del saldo vigente.

La valoracion manual permanece disponible y cambia el costo acumulado sin cambiar stock.

### Ventas

Checkout conserva dos dimensiones independientes:

- Dimension comercial: precio de catalogo, precio aplicado, subtotal, descuento y total cobrado.
- Dimension de costo: costo retirado del inventario y costo acumulado restante.

El costo retirado no depende de:

- Precio actual de venta.
- Precio personalizado aplicado a la linea.
- Descuento global.
- Metodo de pago.

Si una venta contiene varias lineas del mismo producto, el servicio calcula el costo total retirado una sola vez y distribuye los residuos de centavos deterministamente entre las lineas.

### Correccion del precio de una compra

Se podra corregir solamente un movimiento que cumpla:

```text
origin = manual
reason = compra
type = entrada
record_version = 2
```

Solo se modifica el precio de compra.

Permanecen inmutables:

- Producto.
- Presentacion y snapshots de conversion.
- Cantidad capturada.
- Cantidad base.
- Stock antes y despues.
- Fecha del movimiento.
- Proveedor, documento y nota.
- Token original.

No se agrega una tabla de auditoria. Al confirmar, el precio anterior y los snapshots de costo derivados son reemplazados.

La interfaz debe advertir esta consecuencia antes de confirmar.

## Cotizacion de correccion

La correccion usa dos pasos: cotizar y confirmar.

La cotizacion recibe:

- ID del movimiento de compra.
- Nuevo precio de compra.

Rust devuelve:

- Identidad del producto y movimiento.
- Cantidad y forma de captura originales.
- Precio anterior y nuevo.
- Costo original y nuevo de la compra.
- Costo actual del producto antes y despues del replay.
- Cantidad de movimientos posteriores afectados.
- Cantidad de ventas cuyos costos cambian.
- Fecha de la primera y ultima fila afectada.
- Fingerprint opaco del estado cotizado.

La cotizacion no escribe en la base.

El fingerprint incluye como minimo:

- Movimiento objetivo y precio original.
- Nuevo precio normalizado.
- Conversion y cantidades persistidas.
- Stock y costo actuales del producto.
- IDs, orden y snapshots de costo del rango afectado.
- Costos de detalles de venta relacionados.

Si cualquier movimiento, detalle, stock o costo cambia antes de confirmar, la confirmacion exige una nueva cotizacion.

## Replay de costos

La confirmacion abre una transaccion `IMMEDIATE` y bloquea escrituras concurrentes.

El orden total del replay es:

```text
created_at ASC, id ASC
```

El backend:

1. Vuelve a validar que el movimiento objetivo sea una compra v2 elegible.
2. Compara el fingerprint cotizado con el estado vigente.
3. Usa `inventory_cost_before_cents` de la compra como punto inicial.
4. Recalcula `unit_price_cents` y `movement_cost_cents` de la compra.
5. Recorre cronologicamente todos los movimientos posteriores del producto.
6. Valida continuidad de stock antes y despues sin modificar cantidades.
7. Recalcula costos de entradas, salidas y ventas.
8. Actualiza `sale_details.movement_cost_cents` para ventas afectadas.
9. Conserva totales, precios y descuentos comerciales de las ventas.
10. Actualiza `products.inventory_cost_cents` con el saldo final.
11. Confirma todo o revierte todo.

### Entradas posteriores

Las entradas posteriores conservan su precio y costo de movimiento originales. Solo se recalculan sus snapshots de costo acumulado antes y despues.

### Salidas y ventas posteriores

Las salidas se recalculan con el saldo vigente en ese punto del replay.

Para ventas con varias lineas del mismo producto:

- Se agrupan por venta y producto.
- Se calcula el costo total retirado por la cantidad base agregada.
- Se reutiliza el mismo algoritmo productivo de distribucion de residuos.
- Se actualizan movimiento y detalle de venta de forma consistente.

### Valoraciones posteriores

Una valoracion establece un saldo absoluto y funciona como frontera del efecto financiero:

- Su `inventory_cost_before_cents` se actualiza con el saldo recalculado.
- Su valor establecido permanece igual.
- Los movimientos posteriores parten del valor de la valoracion.

### Costos desconocidos

`NULL` nunca se interpreta como cero.

El replay conserva las transiciones de costo desconocido de SQLite 12. Si el rango no contiene informacion suficiente para reproducir una transicion con seguridad, la correccion se rechaza.

### Rechazos del replay

La correccion aborta sin escrituras si:

- El movimiento no es una compra manual v2.
- El precio nuevo no es valido o excede el limite seguro.
- El fingerprint esta obsoleto.
- El rango contiene movimientos v1 que impiden reconstruir el costo con certeza.
- La cadena de `stock_before` y `stock_after` ya es inconsistente.
- Una venta no tiene sus movimientos y detalles relacionados completos.
- El replay produciria costo negativo.
- Cualquier calculo excede el limite seguro de JavaScript.
- Una actualizacion concurrente cambia el producto o el historial.

## Idempotencia de la correccion

No se agrega una tabla ni un token persistido nuevo para correcciones.

La operacion se hace segura ante reintentos mediante:

- Fingerprint del estado previo.
- Precio original esperado.
- Precio nuevo solicitado.
- Comparacion del estado actual antes de escribir.

Si una confirmacion se aplico pero la respuesta se perdio, un reintento con el mismo objetivo y precio debe reconocer que la compra ya contiene el precio solicitado y devolver el resultado vigente si toda la cadena es consistente.

Un precio distinto exige una nueva cotizacion.

## Modelo objetivo

### `BaseUnit`

Rust y TypeScript admiten:

```text
piece | gram | milliliter | centimeter
```

Todos los `match` deben ser exhaustivos. No se permiten ternarios que interpreten cualquier unidad distinta de gramos como mililitros.

### Producto

El producto conserva:

- Identidad y metadatos.
- Unidad base inmutable.
- Stock en unidad base.
- Precio comercial legado sincronizado con la presentacion predeterminada.
- `inventory_cost_cents`.
- Estado de costo conocido o desconocido.
- Costo promedio derivado.

### Presentacion

`product_presentations` mantiene la estructura de SQLite 12:

- `mode` sigue siendo `fixed | measured`.
- `base_quantity` conserva su significado.
- `price` sigue siendo precio al cliente.
- La presentacion predeterminada es unica y activa.
- Una presentacion usada conserva bloqueados `mode` y `base_quantity`.

### Movimiento de inventario

`inventory_movements` mantiene versiones 1 y 2 y todas las columnas de SQLite 12:

- Producto y snapshots de busqueda.
- Presentacion y snapshots de conversion.
- Cantidad capturada y cantidad base.
- Precio de compra cuando corresponde.
- Stock antes y despues.
- Costo del movimiento.
- Costo acumulado antes y despues.
- Proveedor, documento y nota.
- Token de operacion y fingerprint.

No se agregan columnas de importes de inventario calculados con precios al cliente.

### Detalle de venta

`sale_details` conserva:

- Precios comerciales y totales.
- Snapshots de presentacion y conversion.
- `movement_cost_cents`.

## Migracion 013

Crear o reemplazar antes de publicar:

```text
src-tauri/migrations/013_measured_presentations_centimeters.sql
```

No modificar las migraciones `001-012` ya publicadas.

La implementacion local anterior de 013 no se considera publicada y debe ser reemplazada por completo.

### Cambio estructural permitido

El unico cambio de dominio persistido sera agregar `centimeter` a:

- `products.base_unit`.
- `sale_details.base_unit`.
- `inventory_movements.base_unit`.

SQLite obliga a reconstruir las tablas para cambiar sus `CHECK`.

### Reconstruccion de tablas

La migracion reconstruira en orden seguro:

1. `products`.
2. `product_presentations`, para mantener sus claves foraneas hacia la tabla reconstruida.
3. `sale_details`.
4. `inventory_movements`.

Cada tabla conservara literalmente la definicion de SQLite 12 salvo por los tres `CHECK` de unidad.

La copia usara listas explicitas de columnas, nunca `SELECT *`.

Se recrearan todos los indices, claves foraneas, restricciones unicas y secuencias.

### Columnas que deben preservarse

`products` conserva:

```text
inventory_cost_cents
```

`sale_details` conserva:

```text
movement_cost_cents
```

`inventory_movements` conserva:

```text
unit_price_cents
movement_cost_cents
inventory_cost_before_cents
inventory_cost_after_cents
```

La migracion no transforma, recalcula ni normaliza esos valores.

### Versiones admitidas

La politica de apertura sera:

```text
0 nuevo -> crear esquema 13
8       -> backup -> migraciones 9..13 -> validar 13
12      -> backup -> migracion 13 -> validar 13
13      -> validar y abrir
9..11   -> rechazar como version intermedia
otra    -> rechazar como incompatible
```

Se conserva:

- Preflight read-only.
- Comparacion canonica de estructuras 12 y 13.
- Backup consistente antes de cualquier escritura.
- Marcador pendiente durable.
- Manifiesto completado.
- Cuarentena y restauracion.
- Compatibilidad con marcadores de migraciones anteriores.

### Snapshot pre y post

Para una migracion 12 a 13 se comparan literalmente:

- Todos los IDs y relaciones.
- Stock por producto.
- Costo acumulado por producto, incluidos `NULL`.
- Precios y costos de movimientos.
- Costos antes y despues.
- Costos de detalles de venta.
- Precios, subtotales, descuentos y totales comerciales.
- Presentaciones y conversiones.
- Metadatos y timestamps.
- Secuencias SQLite.

La unica diferencia estructural aceptada es `centimeter` en los tres `CHECK`.

## Cambios por capa

### SQLite y apertura

- Reemplazar 013 por una migracion conservadora.
- Mantener esquema objetivo 13.
- Mantener rutas desde 8 y 12.
- Validar que costos y relaciones se preserven literalmente.
- Retirar validaciones que esperen ausencia de columnas de costo.
- Mantener hashes de migraciones 001-012.

### Modelos Rust

- Agregar `BaseUnit::Centimeter`.
- Restaurar campos y estados de costo de SQLite 12.
- Mantener fuera del alta stock y costo inicial.
- Restaurar precio y costo en DTO de movimientos.
- Agregar DTO de cotizacion y confirmacion de correccion.

### Servicios Rust

- Mantener validacion unidad/modo.
- Mantener bases estandar y aritmetica entera.
- Crear productos con stock y costo cero.
- Restaurar aritmetica de costo promedio.
- Restaurar valoracion.
- Restaurar retiro de costo en checkout.
- Implementar replay de correcciones reutilizando las mismas funciones productivas.
- Mantener transacciones inmediatas e idempotencia.

### Contratos TypeScript e IPC

- Agregar `centimeter` a `BaseUnit`.
- Mantener fuera del alta stock y costo inicial.
- Restaurar campos de costo y precio de compra.
- Versionar intents pendientes para incluir precio de compra.
- Agregar contratos de cotizacion y confirmacion de correccion.

### Productos

- Retirar stock inicial y precio de compra inicial.
- Mantener pieza/paquete solo para productos por pieza.
- Usar opciones estandar u otra medida para unidades medidas.
- Mantener estadisticas y columnas de costo.
- Agregar longitud a filtros y estadisticas sin sumar unidades incompatibles.

### Inventario

- Restaurar captura de precio de compra.
- Restaurar valoracion manual.
- Restaurar filtros y mensajes de costo.
- Agregar longitud a cantidades, detalle y estadisticas.
- Agregar accion `Corregir precio de compra` a filas elegibles.
- Mostrar impacto completo antes de confirmar una correccion.

### POS y ventas

- Mantener captura directa para presentaciones medidas.
- No mostrar controles `+/-` en presentaciones medidas.
- Agregar centimetros y formato en metros.
- Restaurar costo retirado y costo por detalle.
- Mantener snapshots y calculos comerciales actuales.

### Intents locales

El intent manual avanzara a una nueva version que incluya:

- Usuario.
- Token de operacion.
- Producto y presentacion.
- Motivo y tipo.
- Cantidad.
- Precio de compra cuando corresponda.
- Proveedor, documento y nota.
- Fingerprint de cotizacion.

Los intents creados por la implementacion local incompatible no deben confirmarse directamente. Deben cargarse solo para contexto y exigir una nueva cotizacion con precio de compra.

### Documentacion

- Reemplazar referencias al plan local descartado.
- Mantener los planes de SQLite 12 como historia y base del modelo de costo.
- Marcar solamente la captura inicial de stock/costo como reemplazada.
- Actualizar QA, release y rollback para SQLite 13 conservador.

## Orden de implementacion

### Bloque 1: reconciliacion del plan y contratos

- Sustituir el plan descartado.
- Definir contratos finales Rust y TypeScript.
- Mantener `measured` para gramos, mililitros y centimetros.
- Restaurar campos de costo sin recuperar stock/costo inicial en Productos.

### Bloque 2: migracion 013 conservadora

- Reescribir 013 desde la definicion canonica de SQLite 12.
- Ampliar solo los `CHECK` de unidad.
- Preservar costos, relaciones, indices y secuencias.
- Adaptar validacion canonica y snapshots pre/post.

### Bloque 3: productos y presentaciones

- Crear siempre con stock y costo cero.
- No crear movimiento inicial.
- Completar selectores kilogramo, litro, metro u otra medida.
- Mantener edicion, default y bloqueos historicos.

### Bloque 4: inventario con costos

- Restaurar DTO, cotizacion y confirmacion v12.
- Restaurar compras, ajustes, valoracion y estadisticas.
- Mantener centimetros extremo a extremo.
- Versionar intents locales.

### Bloque 5: ventas y checkout

- Restaurar retiro proporcional de costo.
- Restaurar distribucion por linea.
- Restaurar snapshots contables en detalles y movimientos.
- Mantener centimetros y calculos comerciales.

### Bloque 6: correccion de compras

- Implementar cotizacion read-only.
- Implementar fingerprint del rango.
- Implementar replay transaccional.
- Actualizar costos de detalles de venta.
- Agregar UI de correccion y estados de error.

### Bloque 7: QA, limpieza y documentacion

- Actualizar fixtures v8 y v12.
- Actualizar runbook de release y rollback.
- Buscar campos comerciales descartados.
- Ejecutar auditorias independientes.
- Ejecutar validacion end-to-end.

## Pruebas requeridas

### Dominio medido

- `piece + fixed` es valido.
- `piece + measured` se rechaza para datos nuevos.
- `gram + measured` es valido.
- `gram + fixed` se rechaza para datos nuevos.
- `milliliter + measured` es valido.
- `milliliter + fixed` se rechaza para datos nuevos.
- `centimeter + measured` es valido.
- `centimeter + fixed` se rechaza.
- Bases estandar 1000, 1000 y 100.
- Otra medida positiva y segura para JavaScript.

### Productos

- Alta no muestra ni envia stock inicial.
- Alta no muestra ni envia precio de compra.
- Producto se crea con stock cero.
- Producto se crea con costo acumulado cero.
- No se crea movimiento inicial.
- Edicion conserva presentaciones no modificadas.
- Conversiones usadas permanecen bloqueadas.

### Calculos comerciales

- 250 g a precio por kilo.
- 500 ml a precio por litro.
- 75 cm a precio por metro.
- Base personalizada de 100 g.
- Base personalizada de 25 cm.
- Redondeo half-up.
- Proteccion de overflow.

### Costos de inventario

- Compra aumenta stock y costo.
- Ajuste de entrada aumenta stock y costo.
- Salida retira costo proporcional.
- Agotar stock deja costo cero.
- Costo desconocido conserva `NULL` correctamente.
- Valoracion cambia costo sin cambiar stock.
- Presentacion fija y medida calculan costo correctamente.
- Cotizacion obsoleta si cambia stock, costo, conversion o precio.
- Movimiento conserva proveedor, documento y nota.

### Ventas

- Checkout descuenta stock y costo.
- Precio personalizado no altera costo retirado.
- Descuento global no altera costo retirado.
- Varias lineas del mismo producto distribuyen residuos.
- `sale_details.movement_cost_cents` coincide con movimientos.
- Venta en centimetros conserva snapshots y costo.
- Fallo intermedio revierte venta, stock y costo.

### Correccion de compras

- Cotizacion no escribe.
- Compra sin movimientos posteriores se corrige.
- Compra con entradas posteriores recalcula saldos.
- Compra con ajustes de salida recalcula costo retirado.
- Compra con una venta posterior actualiza detalle y movimiento.
- Compra con varias lineas de venta distribuye residuos igual que checkout.
- Precio personalizado y descuento comercial permanecen intactos.
- Valoracion posterior corta el efecto sobre el saldo final.
- Costo desconocido no se convierte en cero.
- Movimiento v1 no reconstruible se rechaza.
- Fingerprint obsoleto exige recotizar.
- Reintento de una correccion aplicada no duplica efectos.
- Overflow o inconsistencia revierte toda la transaccion.
- Cantidad, stock y totales comerciales nunca cambian.

### Migracion

- Base nueva termina en version 13.
- Fixture v8 migra hasta 13.
- Fixture v12 migra hasta 13.
- Base v13 se abre sin migrar.
- Versiones 9 a 11 se rechazan.
- Todos los costos conocidos y desconocidos se preservan literalmente.
- IDs, relaciones, stock, ventas y secuencias se preservan.
- No aparecen columnas comerciales descartadas.
- `centimeter` es valido en productos, detalles y movimientos.
- Unidades desconocidas se rechazan.
- Falla posterior conserva backup y marcador recuperable.

### Interfaz

- Selector `Kilogramo / Otra medida`.
- Selector `Litro / Otra medida`.
- Selector `Metro / Otra medida`.
- Campo estandar bloqueado.
- Campo personalizado editable.
- Precio de compra disponible en Inventario, no en Productos.
- Valoracion disponible.
- Filtros y mensajes de costo restaurados.
- Accion de correccion solo en compras elegibles.
- Vista correcta en escritorio y movil.
- Flujo accesible con teclado y foco restaurado.
- Formatos de gramos, kilogramos, mililitros, litros, centimetros y metros.

## Verificacion

Ejecutar como minimo:

```bash
pnpm test
pnpm build
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
cargo test --release --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
pnpm qa:migration-v8-v13
```

Tambien se debe ejecutar el QA actualizado sobre:

- Fixture v8.
- Fixture v12 con costos conocidos y desconocidos.
- Fixture v12 con ventas, ajustes y valoraciones posteriores a una compra corregible.
- Copia cerrada y anonimizada de la base de produccion.

## Criterios de aceptacion

- Gramos, mililitros y centimetros solo crean presentaciones `measured`.
- Kilo, litro y metro fijan sus bases sin permitir edicion.
- Otra medida permite una base entera personalizada.
- Centimetros funcionan de extremo a extremo.
- Crear un producto no solicita stock ni costo inicial.
- Crear un producto guarda stock y costo acumulado cero sin movimiento inicial.
- Inventario solicita precio de compra para entradas.
- Costo promedio, valor a costo, valoracion y costo de ventas funcionan como en SQLite 12.
- No se pierde ninguna columna ni valor de costo al migrar.
- La migracion 013 solo amplia las unidades persistidas con `centimeter`.
- Una compra v2 puede corregir su precio sin cambiar cantidad ni stock.
- La correccion recalcula todos los costos posteriores dependientes.
- Los totales comerciales de ventas nunca cambian durante un replay.
- Un replay inseguro o inconsistente se rechaza sin escrituras parciales.
- Backup, marcador y restauracion funcionan desde esquemas 8 y 12.
- Suite frontend, backend, release y QA terminan correctamente.

## Riesgos y mitigaciones

### Mutacion de historia sin auditoria

La decision confirmada es no agregar una tabla de auditoria. La interfaz debe advertir que el precio anterior sera reemplazado y limitar la accion a usuarios autenticados y movimientos elegibles.

### Recalculo de costos de ventas cerradas

La correccion cambia solamente el costo contable asociado a las ventas. No cambia precios, descuentos, pagos ni totales cobrados.

### Historial insuficiente

Movimientos v1 o cadenas inconsistentes pueden impedir un replay seguro. En ese caso se rechaza la correccion y se usa una valoracion manual para corregir solo el saldo vigente.

### Redondeo acumulado

El replay reutiliza exactamente los helpers productivos de retiro proporcional y distribucion de residuos. No se implementa una segunda formula aproximada.

### Concurrencia

Cotizacion y confirmacion estan separadas. La transaccion inmediata y el fingerprint impiden confirmar sobre un historial modificado.

### Migracion destructiva local descartada

La implementacion local anterior de 013 no debe abrir produccion. Como produccion sigue en SQLite 12, la migracion puede reemplazarse antes de publicar. Si se detectara una base real en version 13, se debe detener el despliegue y recuperar los costos desde su backup v12 antes de continuar.

## Despliegue

1. Reemplazar la migracion 013 antes de abrir cualquier copia de produccion.
2. Ejecutar suites y QA sintetico v8 y v12.
3. Obtener una copia cerrada y anonimizada de produccion.
4. Confirmar `PRAGMA user_version = 12` en la copia.
5. Comparar costos, stock, ventas, detalles, movimientos y secuencias antes de migrar.
6. Ejecutar preflight, backup y migracion sobre la copia.
7. Confirmar igualdad literal de costos y relaciones despues de migrar.
8. Probar productos, centimetros, compras, valoracion, POS y ventas.
9. Probar una correccion de compra sobre otra copia desechable.
10. Probar rollback al backup v12 con la version anterior.
11. Generar y validar el artefacto candidato.
12. Cerrar la aplicacion de produccion antes de instalar.
13. Conservar el backup v12 durante el periodo operativo definido.

La migracion nunca se probara por primera vez sobre el archivo activo de produccion.
