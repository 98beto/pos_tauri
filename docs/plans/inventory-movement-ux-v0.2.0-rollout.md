# Selector de inventario y despliegue seguro de v0.2.0

**Estado:** implementacion local hasta el bloque 12 preparada para `v0.2.0`. El bloque 11 y la documentacion operativa de los bloques 13-14 estan completados; siguen pendientes la validacion con una copia cerrada real, `mode=validate`, la prueba de artefactos y rollback, el tag y `mode=publish`. No se ha migrado produccion ni publicado esta version.

## Objetivo

Simplificar el registro manual de inventario y preparar una actualizacion segura desde la version actualmente publicada en `main` hacia la funcionalidad de presentaciones e inventario medido.

Resultados esperados:

- Reemplazar terminos ambiguos al configurar presentaciones.
- Sustituir el buscador y selector separados del movimiento manual por un unico buscador de productos.
- Explicar la captura por pieza, paquete, peso o volumen con opciones visibles y ejemplos.
- Mantener la cotizacion y confirmacion atomica ya implementadas.
- Publicar los cambios como `v0.2.0`.
- Terminar primero todas las funciones, ajustes visuales y contratos de venta sin migrar produccion.
- Crear un backup consistente antes de migrar una base de produccion de esquema 8 al esquema final 12.
- Permitir un rollback operativo restaurando el backup y la version anterior.

## Estado de produccion confirmado

La rama `main` actualmente declara:

- Aplicacion `v0.1.0`.
- Migraciones `001` a `008`.
- Ultima version esperada de SQLite: `PRAGMA user_version = 8`.

El repositorio de trabajo ya declara `v0.1.1`, pero esa no es la version confirmada en produccion. En este plan:

- Version instalada de produccion y objetivo de rollback: `v0.1.0` desde `main`.
- Version intermedia del repositorio: `v0.1.1`, no desplegada como parte de este plan.
- Version final a publicar: `v0.2.0`.

La version planificada queda compuesta por:

- `009_create_product_presentations.sql`.
- `010_rebuild_sale_details.sql`.
- `011_rebuild_inventory_movements.sql`.
- `012_add_sale_pricing_and_discounts.sql`.
- Ultima version esperada: `PRAGMA user_version = 12`.

La ruta de actualizacion de produccion es:

```text
v0.1.0 / esquema 8
        ->
v0.2.0 / esquema 12
```

Esto es valido siempre que la base provenga de una compilacion normal de `main` y no haya sido abierta previamente por una compilacion intermedia que ya marque `user_version = 9`, `10`, `11` o `12`.

## Orden obligatorio de trabajo

La base de produccion no se migrara mientras se esten definiendo o corrigiendo funciones y pantallas.

El orden sera:

1. Terminar textos y configuracion de presentaciones.
2. Terminar el nuevo selector de movimientos de inventario.
3. Terminar el modal de cantidad al agregar al carrito.
4. Terminar precios personalizados por linea y descuento global.
5. Completar pruebas frontend y backend usando bases temporales y copias de desarrollo.
6. Congelar el contrato final de ventas, carrito e inventario.
7. Finalizar y revisar las migraciones `009` a `012`.
8. Probar la migracion completa sobre una copia real del esquema 8.
9. Crear el candidato firmado `v0.2.0`.
10. Migrar produccion una sola vez, al instalar `v0.2.0`.

Durante los bloques 1 a 6 no se debe abrir la base de produccion con una compilacion de esta rama. Las pruebas de migracion se ejecutan solamente sobre fixtures, archivos temporales o copias cerradas de la base.

## Decisiones de producto

### Forma de venta

Reemplazar las opciones actuales:

- `Cantidad definida`.
- `Cantidad capturada al vender`.

Por:

- `Por unidades o paquetes`.
- `Por peso o volumen exacto`.

La pregunta visible sera:

```text
Como se captura la cantidad al vender?
```

Ayudas:

```text
Por unidades o paquetes
Ejemplos: piezas, bolsas, cajas o botellas.
Cada unidad descuenta una cantidad fija del inventario.
```

```text
Por peso o volumen exacto
El cajero captura directamente los gramos o mililitros vendidos.
```

Los campos dinamicos seran:

- Unidades o paquetes: `Cuanto contiene cada unidad o paquete?`.
- Peso o volumen: `A que cantidad corresponde este precio?`.

No se modifica el esquema por este cambio. Las opciones siguen correspondiendo a:

```text
product_presentations.mode = fixed | measured
product_presentations.base_quantity
```

### Costo de inventario

No se agregara un campo de ultimo costo al producto.

Cada movimiento conserva:

- `unit_price_cents`: precio aplicado.
- `movement_cost_cents`: importe total del movimiento.
- `inventory_cost_before_cents`: costo acumulado anterior.
- `inventory_cost_after_cents`: costo acumulado posterior.
- Proveedor, documento y nota.

El producto conserva:

```text
products.inventory_cost_cents
```

Este valor representa el costo total acumulado del inventario actual, no el ultimo precio de compra.

El costo promedio mostrado se obtiene de:

```text
inventory_cost_cents / stock
```

Si se necesita consultar el ultimo precio de compra, se obtiene del movimiento de compra mas reciente. No se duplica en `products`.

## Bloque 1: textos de presentaciones

Modificar `src/features/products/ProductsView.tsx`.

Cambios:

- Cambiar las etiquetas del selector de forma de venta.
- Cambiar el titulo a `Como se captura la cantidad al vender?`.
- Mostrar una ayuda breve debajo de cada opcion.
- Mantener oculto el modo medido para productos cuya unidad base sea `piece`.
- Mantener bloqueados modo y cantidad cuando la presentacion tenga historia.
- Conservar editables nombre, precio, codigo de barras y actividad.

Pruebas:

- Seleccion de unidades o paquetes genera `mode = fixed`.
- Seleccion de peso o volumen genera `mode = measured`.
- Las etiquetas dinamicas coinciden con el modo.
- Una presentacion historica conserva bloqueada su conversion.

## Bloque 2: busqueda dedicada de productos

### Backend

Agregar una consulta autenticada dedicada al formulario de movimientos:

```text
search_inventory_products
```

Entrada:

```text
search: texto opcional
page: entero positivo
```

Salida:

```text
items
page
total
page_size = 20
```

Cada resultado debe incluir:

- ID.
- Nombre actual.
- SKU actual.
- Stock actual.
- Unidad base.
- Estado de costo.
- Presentaciones activas.

La consulta debe:

- Buscar por nombre y SKU normalizados.
- Ordenar de forma estable por nombre e ID.
- Aplicar `LIMIT` y `OFFSET` en SQLite.
- No consultar movimientos ni estadisticas globales.
- No cargar el catalogo completo para recortarlo en React.
- Ordenar por `name_search_key, id` para mantener paginas estables.

Archivos esperados:

- `src-tauri/src/models/inventory_movement.rs`.
- `src-tauri/src/services/inventory.rs`.
- `src-tauri/src/commands/inventory.rs`.
- `src-tauri/src/lib.rs`.
- `src/types/inventory_movement.ts`.
- `src/api/inventory.ts`.

### Frontend

Eliminar del modal la combinacion de:

- Campo `Buscar producto`.
- Desplegable `Producto`.
- Botones manuales de pagina.

Crear un unico selector buscable:

1. El usuario escribe nombre o SKU.
2. La busqueda se ejecuta despues de una pausa breve.
3. Se muestran los primeros 20 resultados.
4. Cada resultado muestra nombre, SKU, stock y unidad.
5. Al seleccionar uno, el buscador se reemplaza por una tarjeta del producto.
6. La tarjeta ofrece la accion `Cambiar producto`.
7. Si existen mas resultados, aparece un boton accesible `Cargar mas` dentro del listbox.

`Cargar mas` solicita la pagina siguiente y agrega resultados sin reemplazar los anteriores. Cambiar el texto reinicia la pagina y descarta logicamente respuestas anteriores. Una pagina fuera de rango devuelve una lista vacia sin perder la seleccion actual.

Estados requeridos:

- Inicial.
- Buscando.
- Resultados.
- Sin resultados.
- Error con reintento.
- Datos anteriores durante una recarga.

Accesibilidad:

- Patron combobox/listbox.
- Flechas para recorrer resultados.
- `Enter` para seleccionar.
- `Escape` para cerrar resultados.
- Foco visible.
- Nombre, SKU y stock disponibles para lectores de pantalla.

## Bloque 3: forma de capturar la cantidad

Eliminar el desplegable tecnico:

```text
Presentacion o unidad base
```

Mostrar la pregunta:

```text
Como estas contando esta cantidad?
```

Las respuestas se muestran como tarjetas seleccionables.

### Producto por piezas

Ejemplo de unidad base:

```text
Piezas individuales
La cantidad se registra directamente en piezas.
```

Ejemplo de presentacion fija:

```text
Bolsa 100
Cada bolsa agrega o retira 100 piezas.
```

### Producto por peso

Ejemplo de presentacion fija:

```text
Bolsa 250 g
La cantidad indica cuantas bolsas recibiste o retiraste.
```

Ejemplo de presentacion medida:

```text
Peso exacto en gramos
Captura directamente los gramos.
El precio se registra por la base configurada, por ejemplo 1 kilogramo.
```

### Producto por volumen

Ejemplo de presentacion fija:

```text
Botella de 500 ml
Cada botella agrega o retira 500 mililitros.
```

Ejemplo de presentacion medida:

```text
Volumen exacto en mililitros
Captura directamente los mililitros.
```

### Evitar opciones duplicadas

Si existe una presentacion fija equivalente a la unidad base, por ejemplo `Pieza` con factor `1`, no mostrar simultaneamente:

- `Pieza`.
- `Captura directa en unidad base`.

Mostrar una sola opcion operativa, por ejemplo `Piezas individuales`, y conservar internamente el `presentation_id` cuando corresponda.

Para productos por peso o volumen, la tarjeta debe indicar claramente si el precio capturado corresponde a:

- Cada paquete.
- Cada gramo o mililitro.
- La base configurada, por ejemplo 1,000 gramos o 1,000 mililitros.

## Bloque 4: cotizacion y confirmacion

Conservar el flujo actual de dos pasos:

1. Calcular importe.
2. Revisar y confirmar movimiento.

El resumen debe mostrar:

```text
Producto: Tornillo
Forma de captura: Bolsa 100
Cantidad recibida: 10 bolsas
Entrada al inventario: 1,000 piezas
Precio de compra: $50 por bolsa
Importe total: $500
```

Extender `InventoryMovementQuote` para que el resumen use exclusivamente datos resueltos por Rust:

- ID, nombre y SKU actuales del producto.
- ID y nombre de presentacion cuando exista.
- Unidad base.
- Modo y factor/base de precio.
- Cantidad ingresada normalizada.
- Cantidad base aplicada.
- Precio unitario en centavos y descripcion de su base.
- Importe total.
- Fingerprint opaco de la cotizacion.

El frontend no debe combinar una cotizacion vigente con nombres, factores o precios tomados de un estado local anterior.

Reglas:

- Cambiar producto invalida la cotizacion.
- Cambiar forma de captura invalida la cotizacion.
- Cambiar operacion, cantidad o precio invalida la cotizacion.
- La confirmacion envia el fingerprint opaco de la cotizacion.
- Rust vuelve a calcular dentro de una transaccion `IMMEDIATE`.
- Si la cotizacion cambio, no se escribe y se solicita una nueva confirmacion.
- Los intents pendientes conservan token, producto, presentacion y metadatos.

## Bloque 5: cantidad al agregar al carrito

Actualmente una presentacion fija se agrega con cantidad `1` y el usuario debe pulsar `+` repetidamente. Cambiar este flujo para que todas las presentaciones soliciten cantidad antes de agregarse.

### Apertura del modal

Al seleccionar una presentacion desde el buscador del POS:

- No agregarla inmediatamente.
- Abrir el editor de cantidad existente.
- Usar el mismo patron visual para cantidades fijas y medidas.
- Cerrar el selector de productos antes de mostrar el editor.

Para una presentacion fija mostrar, por ejemplo:

```text
Producto: Tornillo
Presentacion: Bolsa 100
Cantidad de bolsas: [ 10 ]
Disponible: 12 bolsas / 1,250 piezas
```

Para una pieza individual:

```text
Producto: Tornillo
Presentacion: Pieza
Cantidad de piezas: [ 25 ]
Disponible: 1,250 piezas
```

Para una presentacion medida conservar el flujo actual:

```text
Producto: Cafe
Cantidad en gramos: [ 250 ]
Disponible: 3.5 kg
```

### Reglas

- Aceptar solamente enteros positivos.
- Validar demanda agregada de todas las presentaciones del mismo producto.
- Usar una sola llamada atomica `set_cart_item_quantity`.
- No ejecutar una llamada IPC por cada pieza, gramo o mililitro.
- Si la presentacion ya esta en el carrito, abrir el modal con la cantidad actual.
- Guardar reemplaza la cantidad; no la suma implicitamente.
- Un fallo no debe modificar parcialmente el carrito.
- Mantener botones `+/-` como atajo para presentaciones fijas, pero no como unico metodo para capturar cantidades grandes.
- Las presentaciones medidas siguen usando `Editar cantidad` y no muestran `+/-` por gramo o mililitro.

### Pruebas

- Agregar 25 piezas usa una sola operacion.
- Agregar 10 bolsas de 100 consume 1,000 piezas.
- Editar una linea existente reemplaza la cantidad.
- Cancelar no modifica el carrito.
- Cantidad vacia, cero, decimal, con signo o fuera de stock se rechaza.
- Pieza y bolsa del mismo producto respetan la disponibilidad agregada.
- Un error IPC conserva el carrito anterior.

## Bloque 6: precio aplicado por linea

Permitir cambiar el precio unitario de una linea solamente dentro del carrito y de la venta actual.

La modificacion nunca debe actualizar:

```text
product_presentations.price
products.price
```

### Estado del carrito

Cambiar el estado interno de lineas de:

```text
BTreeMap<presentation_id, entered_quantity>
```

A una entrada que conserve:

```text
presentation_id
entered_quantity
unit_price_override: Option<i64>
```

El estado completo de sesion debe separar lineas y descuento:

```text
SessionCart {
  lines: BTreeMap<presentation_id, CartEntry>,
  discount: CartDiscount
}
```

Rust debe resolver para cada linea:

- `catalog_unit_price`: precio vigente de la presentacion.
- `applied_unit_price`: precio de catalogo o precio personalizado.
- `price_overridden`: indica si existe cambio temporal.
- `line_total`: total calculado con el precio aplicado.

Extender `CartLine` con esos cuatro campos y conservar `unit_price` solamente si se renombra o documenta de forma inequívoca como precio aplicado. Evitar dos campos publicos con semantica ambigua.

Agregar comandos autenticados:

```text
set_cart_item_unit_price(presentation_id, unit_price)
reset_cart_item_unit_price(presentation_id)
```

Cada comando debe construir y validar una copia candidata del carrito. El estado de sesion se reemplaza solamente si toda la operacion termina correctamente; un error de parseo, overflow o disponibilidad conserva lineas, cantidades, precios y descuento anteriores.

### Interfaz

Agregar una accion `Cambiar precio` en cada linea.

El modal debe mostrar:

```text
Precio de catalogo: $80 por Bolsa 100
Precio aplicado a esta venta: [ 75.00 ]
```

Acciones:

- `Guardar precio`.
- `Restablecer precio de catalogo`.
- `Cancelar`.

Reglas:

- El precio aplicado debe ser mayor que cero.
- Se captura como texto decimal y Rust lo convierte a centavos.
- Para presentaciones medidas representa el precio por `base_quantity`, por ejemplo por kilogramo o litro.
- Cambiar el precio recalcula el total de linea y el carrito.
- El cambio forma parte del fingerprint del carrito.
- Cambiar el precio de catalogo mientras existe el carrito invalida cualquier fingerprint anterior.
- Vaciar el carrito elimina todos los precios personalizados.
- Completar la venta no modifica el producto ni la presentacion.

No se agregan roles ni permisos en este alcance. El proyecto tiene un unico usuario autenticado que administra el negocio, por lo que cualquier sesion valida puede cambiar el precio y aplicar descuentos.

## Bloque 7: descuento global de la venta

Permitir un descuento global despues de calcular las lineas.

Tipos admitidos:

- Porcentaje.
- Importe fijo.

### Aritmetica

Orden de calculo:

```text
1. Resolver precio aplicado de cada linea.
2. Calcular total de cada linea.
3. subtotal = suma de lineas.
4. Calcular descuento global.
5. total = subtotal - descuento.
```

Para porcentaje:

- Capturar hasta dos decimales.
- Convertir a puntos base enteros; `10.00% = 1000`.
- Calcular con `i128` y redondeo half-up.
- No usar `f32` ni `f64`.
- Aceptar entre `0.01%` y `99.99%`, representado como `1..9999` puntos base.
- Rechazar un porcentaje positivo cuyo importe redondee a cero centavos; no guardar descuentos visualmente activos sin efecto monetario.

Para importe fijo:

- Parsear texto monetario a centavos.
- Debe ser mayor que cero.

Reglas comunes:

- El descuento no puede superar ni igualar el subtotal; la venta debe conservar total positivo.
- Un carrito vacio no admite descuento.
- El descuento se elimina al vaciar o completar el carrito.
- Cambiar lineas, cantidades o precios recalcula el importe del descuento porcentual.
- Un descuento fijo que deje de ser valido bloquea checkout hasta corregirlo.
- El costo de inventario retirado no cambia por el descuento comercial.
- El descuento y sus valores forman parte del fingerprint e idempotencia.
- Un valor cero se normaliza quitando el descuento, nunca como descuento porcentual o fijo activo.

Representacion canonica:

```text
none:
  discount_value = 0
  discount_amount = 0

percentage:
  discount_value = puntos base entre 1 y 9999
  discount_amount = round_half_up(subtotal * discount_value / 10000)

fixed:
  discount_value = centavos positivos
  discount_amount = discount_value
```

### Interfaz

Agregar en el resumen del carrito:

```text
Subtotal                 $1,000.00
Descuento 10%              -$100.00
Total                      $900.00
```

Agregar una accion `Aplicar descuento` con:

- Selector `Porcentaje` o `Importe fijo`.
- Campo del valor.
- Vista previa calculada por Rust.
- Accion para quitar el descuento.

Agregar comandos autenticados:

```text
set_cart_discount(discount_type, value)
remove_cart_discount()
```

Extender `Cart` con:

- `discount_type`.
- `discount_value` normalizado.
- `discount_amount`.
- `subtotal` antes del descuento.
- `total` definitivo.

Estas mutaciones tambien deben validar una copia candidata y reemplazar el estado solamente al exito.

El efectivo recibido y el cambio deben usar el total final despues del descuento.

## Bloque 8: persistencia de precios y descuentos

Estas funciones requieren persistencia historica. Agregar:

```text
012_add_sale_pricing_and_discounts.sql
```

`012` debe reconstruir `sales`, `sale_details` e `inventory_movements`; no basta con agregar columnas sueltas porque se requieren restricciones `NOT NULL`, checks cruzados, una nueva version de detalle y existen claves foraneas desde movimientos hacia detalles.

Orden de la migracion:

1. Renombrar primero `inventory_movements`, despues `sale_details` y finalmente `sales` a tablas temporales.
2. Crear la nueva tabla `sales` con todas sus restricciones.
3. Crear la nueva tabla `sale_details` admitiendo `record_version IN (1, 2, 3)`.
4. Crear nuevamente `inventory_movements` con sus restricciones y FKs actuales.
5. Copiar ventas preservando IDs, tokens, fingerprint, usuario, efectivo, cambio, fechas y total.
6. Copiar detalles preservando IDs, snapshots, cantidades, costos y fechas.
7. Copiar movimientos preservando todos sus campos y relaciones `sale_id`/`sale_detail_id`, sin recalcular stock ni costo.
8. Transferir `sqlite_sequence` de las tres tablas sin reutilizar IDs eliminados.
9. Eliminar tablas temporales en orden hijo a padre solamente despues de copiar todo.
10. Recrear indices y claves unicas aplicables.
11. Validar `foreign_key_check` en pruebas antes de cerrar el bloque.

La migracion completa `009` a `012` sigue ejecutandose atomicamente mediante `rusqlite_migration`; cualquier fallo revierte a esquema 8.

### Ventas

Agregar a `sales`:

- `subtotal`: suma de lineas antes del descuento global.
- `discount_type`: `none`, `percentage` o `fixed`.
- `discount_value`: puntos base para porcentaje o centavos para importe fijo.
- `discount_amount`: importe final descontado en centavos.
- Mantener `total` como total definitivo cobrado.

Restricciones:

```text
subtotal > 0
discount_amount >= 0
total = subtotal - discount_amount
total > 0
```

Checks por tipo:

- `none`: valor e importe iguales a cero.
- `percentage`: valor entre 1 y 9999, con importe positivo y menor al subtotal.
- `fixed`: valor positivo en centavos e importe igual al valor.
- En todos los casos `0 <= discount_amount < subtotal`.

La igualdad matematica de un porcentaje se valida en Rust con `i128` y se persiste como snapshot definitivo; SQLite valida dominios y coherencia estructural, sin usar punto flotante.

Backfill de ventas historicas:

```text
subtotal = total
discount_type = none
discount_value = 0
discount_amount = 0
```

### Detalles de venta

Conservar en cada linea:

- `catalog_unit_price`: snapshot del precio de la presentacion antes del cambio temporal.
- `unit_price`: precio realmente aplicado a la venta.
- `line_total`: total definitivo de linea antes del descuento global.

Agregar `record_version = 3` para todas las ventas nuevas con precios de catalogo/aplicados y descuentos compatibles.

Para version 3 exigir:

- Presentacion y snapshots completos.
- Cantidad ingresada y cantidad base.
- `catalog_unit_price` positivo.
- `unit_price` aplicado positivo.
- `line_total` positivo.

Las versiones 1 y 2 permanecen legibles y no se reinterpretan como version 3.

Para historia legada:

- `catalog_unit_price = unit_price` cuando sea representable.
- No recalcular ni modificar el total historico de la venta.
- Mantener nullable cualquier total legado que no sea representable de forma segura.

### Lectura historica

Tickets y ventas deben mostrar:

- Precio aplicado.
- Precio de catalogo cuando haya diferencia.
- Subtotal.
- Tipo y valor del descuento.
- Importe descontado.
- Total cobrado.

Para ventas nuevas version 3, la suma comprobada debe cumplir:

```text
sum(line_total) = sale.subtotal
sale.subtotal - sale.discount_amount = sale.total
```

Para ventas legadas, exigir la igualdad solamente cuando todas sus lineas tengan `line_total` representable. Si alguna linea legada tiene total nulo por overflow historico, conservar `subtotal = total` de la venta y marcar la suma de lineas como no verificable, sin inventar ni recalcular importes.

## Bloque 9: fingerprint, checkout e idempotencia

Crear una nueva version del fingerprint del carrito que incluya:

- Presentacion.
- Cantidad ingresada.
- Cantidad base.
- Modo y factor.
- Precio de catalogo.
- Precio aplicado.
- Total de linea.
- Tipo de descuento.
- Valor del descuento.
- Importe descontado.
- Subtotal y total final.

Versionar tambien `PendingCheckout` para distinguir el nuevo contrato. El intent local conserva:

- Usuario.
- Token de checkout.
- Fingerprint opaco.
- Metodo de pago.
- Efectivo recibido cuando corresponda.

No necesita duplicar lineas, precios ni descuento porque el fingerprint es la identidad canonica; esos datos se persisten en la venta confirmada.

Checkout debe recargar presentaciones y volver a calcular todo dentro de una transaccion `IMMEDIATE`.

Si cambia precio, cantidad, presentacion o descuento desde la cotizacion:

- Rechazar el fingerprint anterior.
- No insertar una venta parcial.
- No descontar stock ni costo.
- Solicitar confirmacion con los valores nuevos.

Mantener lectura de ventas y retries historicos sin reinterpretarlos como el formato nuevo.

Reglas de retry:

- Si el token ya existe y el carrito esta vacio tras reiniciar, devolver el recibo persistido cuando usuario, pago, efectivo y fingerprint coincidan.
- Si el mismo token llega con otro precio aplicado, descuento, cantidad, usuario, pago o efectivo, devolver `CHECKOUT_TOKEN_REUSED`.
- Para ventas version 3, reconstruir la identidad usando precios de catalogo/aplicados persistidos, cantidades, conversiones, descuento, subtotal y total.
- Mantener ramas explicitas para fingerprints historicos; una version desconocida nunca cae en una rama legada.
- El carrito se limpia solamente despues de recibir confirmacion exitosa o recuperar una venta ya confirmada.

## Bloque 10: backup automatico previo a la migracion

### Dependencia

Habilitar la feature `backup` de `rusqlite` en `src-tauri/Cargo.toml`.

Usar la API de backup de SQLite, no una copia cruda de `pos.db`. Esto permite obtener una copia consistente aunque la base use WAL.

### Preflight

Antes de ejecutar `migrations::run`:

1. Consultar primero el marcador durable de migracion pendiente.
2. Si existe, bloquear la apertura incluso cuando la base indique version 12 y mostrar la ruta exacta del backup asociado.
3. Leer `PRAGMA user_version` cuando no exista marcador pendiente.
4. Si la version es `8`, ejecutar `PRAGMA quick_check`.
5. Ejecutar `PRAGMA foreign_key_check`.
6. Rechazar la actualizacion si alguna comprobacion falla.
7. Crear un backup consistente y unico para este intento.
8. Crear el marcador con la ruta exacta de ese backup.
9. Aplicar `009` a `012` en una sola ejecucion atomica.
10. Confirmar `user_version = 12`.
11. Ejecutar nuevamente `quick_check` y `foreign_key_check`.
12. Publicar y sincronizar un manifiesto completado independiente que conserve la ruta exacta del backup; retirar el marcador pendiente y sincronizar nuevamente el directorio solo despues de que el completado sea durable.
13. Abrir la aplicacion solamente si todas las comprobaciones pasan.

Reestructurar `open_database` para ejecutar deteccion, validacion y backup antes de activar WAL y antes de llamar a `migrations::run`.

Matriz cerrada de versiones:

- Archivo inexistente o base nueva: crear directamente esquema 12 sin backup v8.
- Version 8: validar, crear backup y migrar atomicamente a 12.
- Version 12 sin marcador pendiente: ejecutar comprobaciones de apertura y continuar sin otro backup v8; si existe un manifiesto completado, conservarlo para rollback.
- Version 12 con marcador pendiente: bloquear y exigir restauracion o intervencion controlada.
- Versiones 9, 10 u 11: detener con diagnostico de compilacion intermedia; no continuar automaticamente en produccion.
- Version mayor que 12, negativa o invalida: rechazar como base incompatible.

No crear un backup de migracion para:

- Una base nueva sin datos.
- Una base que ya este correctamente en version 12.

### Archivo de backup

Nombre sugerido:

```text
pos-v8-before-v0.2.0-<timestamp-o-id>.db
```

Requisitos:

- Guardar junto a los datos de la aplicacion.
- Usar permisos `0600` en Unix.
- No sobrescribir silenciosamente un backup existente.
- Mantener el backup despues de una migracion exitosa.
- No eliminar ni modificar la base original si falla el backup.
- Antes de usarlo como respaldo valido, comprobar dentro del backup `user_version = 8`, `quick_check = ok`, FKs validas y presencia de las tablas principales.
- Crear un backup nuevo con nombre unico en cada intento iniciado desde version 8; no reutilizar backups anteriores automaticamente.
- Registrar en el marcador durable la ruta exacta del backup creado para ese intento.
- El marcador debe escribirse atomicamente y nunca apuntar a un archivo parcial.
- Cambiar el marcador a estado completado solamente despues de validar esquema 12, integridad y claves foraneas.
- El manifiesto completado conserva version origen, version destino, fecha y ruta exacta del backup para un rollback posterior.
- Si coexisten marcador activo y manifiesto completado tras un corte, comprobar que coinciden, volver a sincronizar el completado y su directorio y solo entonces retirar el activo. Una discrepancia o un fallo de sync bloquea la apertura, tambien en Windows.

### Errores

Si falla el backup:

- No ejecutar migraciones.
- Mostrar un error seguro.
- Conservar intacta la base original.

Si falla una migracion:

- `rusqlite_migration` revierte atomicamente las migraciones pendientes.
- La base debe permanecer en version 8.
- El backup debe conservarse.
- La aplicacion no debe continuar con un esquema parcial.

Si falla la validacion posterior:

- No abrir la interfaz operativa.
- Conservar la base migrada y el backup para diagnostico.
- Indicar que se debe restaurar el backup antes de volver a `v0.1.0`.
- Mantener el marcador de migracion pendiente para impedir que el siguiente inicio abra la base como si hubiera sido validada.

## Bloque 11: version v0.2.0

Actualizar de forma consistente:

- `package.json`.
- `src-tauri/Cargo.toml`.
- `src-tauri/tauri.conf.json`.
- Pruebas y fixtures de version.
- Lockfiles solamente si las herramientas los actualizan.

Version objetivo:

```text
v0.2.0
```

No publicar este cambio reutilizando `v0.1.0` ni una etiqueta existente.

Despues de publicar `v0.2.0`:

- No modificar las migraciones `009`, `010`, `011` ni `012`.
- Cualquier cambio posterior de esquema debe agregarse como `013` o superior.

## Bloque 12: pruebas automatizadas

### Busqueda y formulario

- Buscar por nombre.
- Buscar por SKU.
- Respuestas fuera de orden.
- Error y reintento.
- Sin resultados.
- Seleccion y cambio de producto.
- Navegacion completa por teclado.
- Producto con una sola presentacion.
- Producto con varias presentaciones.
- Eliminacion de opciones duplicadas.

### Cantidades

- Pieza individual.
- Bolsa de 100 piezas.
- Bolsa de 250 gramos.
- Peso exacto en gramos.
- Botella de 500 mililitros.
- Volumen exacto en mililitros.
- Cantidad vacia, cero, decimal, con signo y fuera de stock.
- Modal de cantidad para presentaciones fijas y medidas.
- Una sola operacion atomica al capturar cantidades grandes.
- Edicion reemplaza la cantidad existente sin sumas accidentales.

### Precios y descuentos

- Precio personalizado por pieza.
- Precio personalizado por paquete.
- Precio personalizado por kilogramo o litro.
- Restablecer precio de catalogo.
- El precio temporal no modifica `product_presentations` ni `products`.
- Descuento porcentual con redondeo half-up.
- Descuento porcentual con hasta dos decimales.
- Descuento por importe fijo.
- Descuento igual o mayor al subtotal rechazado.
- Descuento porcentual recalculado al cambiar lineas.
- Descuento fijo invalido bloquea checkout.
- Efectivo y cambio usan el total descontado.
- Fingerprint cambia con precio o descuento.
- Retry no duplica venta, stock ni costo.
- Ticket historico conserva precio de catalogo, precio aplicado y descuento.
- Restricciones SQL rechazan combinaciones invalidas de tipo, valor e importe.
- Porcentaje minimo, `99.99%`, overflow y descuento que redondea a cero.
- Cambio de precio de catalogo durante checkout, con y sin override.
- Mismo token con distinto override, descuento, efectivo o usuario.
- Retry tras reinicio con venta confirmada y carrito vacio.
- Logout, vaciado y venta exitosa eliminan override y descuento.
- Error IPC al cambiar precio o descuento conserva el carrito completo anterior.

### Costos

- Precio por pieza.
- Precio por paquete.
- Precio por kilogramo.
- Precio por litro.
- Importe total confirmado por Rust.
- Movimiento conserva precio, importe, proveedor, documento y nota.
- El producto conserva costo acumulado, no ultimo costo.
- Producto sin costo permanece con costo desconocido hasta valoracion.

### Backup y migracion

- Base v8 real cerrada y reabierta.
- Backup contiene esquema 8 y datos completos.
- Migracion resultante queda en version 12.
- IDs, stock, ventas, totales, fechas y tokens permanecen iguales.
- `foreign_key_check` queda vacio.
- Falla del backup no inicia la migracion.
- Falla en `009`, `010`, `011` o `012` revierte a version 8.
- Archivo de backup no se sobrescribe.
- Base version 12 no vuelve a ejecutar migraciones ni crea otro backup v8.
- Binario antiguo rechaza una base version 12 como demasiado adelantada.
- Migracion v8 a v12 conserva efectivo, cambio, indices, timestamps, tokens y `sqlite_sequence`.
- Venta legada con `line_total = NULL` conserva el total historico sin exigir suma de lineas.
- Esquemas 9, 10 y 11 se rechazan con diagnostico y sin escrituras.
- Esquema mayor que 12 se rechaza.
- Backup existente valido puede reusarse de forma controlada.
- Backup corrupto o ajeno se rechaza.
- Permisos del backup son `0600` en Unix.
- Fallo de validacion posterior conserva el marcador y bloquea apertura operativa.

## Bloque 13: validacion sobre una copia de produccion

Antes del despliegue:

1. Cerrar completamente la aplicacion de produccion.
2. Obtener una copia consistente de `pos.db`.
3. Ejecutar `v0.2.0` sobre la copia.
4. Confirmar que el backup v8 fue creado.
5. Comparar conteos de productos, ventas, detalles y movimientos.
6. Comparar stock por producto antes y despues.
7. Verificar tickets historicos.
8. Ejecutar `foreign_key_check`.
9. Cerrar y reabrir la aplicacion migrada.
10. Probar compra, venta, ajuste y valoracion.

La copia usada para la prueba no debe contener datos personales si se comparte fuera del equipo autorizado.

## Bloque 14: despliegue y rollback

### Despliegue

1. Congelar UI, contratos, migraciones y commit candidato.
2. Ejecutar el workflow de release en modo `validate` para ese commit exacto.
3. Probar los artefactos candidatos sobre una copia v8 y ejecutar el rollback completo.
4. Corregir cualquier defecto antes de etiquetar; si cambia el commit, repetir validacion y prueba.
5. Crear la etiqueta inmutable `v0.2.0` sobre el commit validado.
6. Ejecutar el modo `publish` y verificar firmas y artefactos finales.
7. Instalar primero el artefacto final publicado en un equipo controlado.
8. Confirmar la creacion y validez del backup.
9. Confirmar que la base quedo en version 12.
10. Ejecutar los escenarios principales de venta e inventario.
11. Reiniciar la aplicacion y repetir comprobaciones.
12. Liberar al resto de equipos despues de validar el primero.

El binario que migra produccion debe provenir del mismo commit firmado y publicado que operara despues de la migracion. No usar un candidato diferente para migrar y despues sustituirlo por otro build.

### Rollback

El binario `v0.1.0` no debe abrir la base version 12. La biblioteca de migraciones la considera adelantada.

Para regresar:

1. Cerrar completamente `v0.2.0`.
2. Conservar una copia de diagnostico de la base version 12.
3. Leer del marcador pendiente o manifiesto completado la ruta exacta `pos-v8-before-v0.2.0-<timestamp-o-id>.db`, verificar nuevamente ese archivo y restaurarlo como `pos.db`.
4. Retirar archivos WAL/SHM residuales solamente con la aplicacion cerrada.
5. Instalar nuevamente `v0.1.0`.
6. Confirmar `PRAGMA user_version = 8`.
7. Verificar productos, stock y ventas antes de reanudar operaciones.

Las operaciones creadas despues de actualizar a `v0.2.0` no existiran en el backup v8 restaurado. Por eso el rollback debe decidirse antes de continuar operando durante un periodo prolongado.

## Verificacion final

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo check --locked --release --manifest-path src-tauri/Cargo.toml
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --release --manifest-path src-tauri/Cargo.toml
pnpm test
pnpm build
```

## Criterios de aceptacion

- Las opciones visibles son `Por unidades o paquetes` y `Por peso o volumen exacto`.
- El formulario de movimiento usa un unico buscador de productos.
- No existe un desplegable separado que obligue a buscar y despues seleccionar.
- La forma de captura se elige mediante tarjetas con conversiones explicitas.
- El usuario ve cantidad base e importe antes de confirmar.
- Agregar cualquier presentacion al carrito solicita primero la cantidad.
- Las cantidades grandes se guardan con una sola mutacion atomica.
- Cada linea permite un precio temporal sin modificar el catalogo.
- El carrito admite descuento global porcentual o fijo.
- El ticket conserva subtotal, descuento, precio de catalogo y precio aplicado.
- Rust sigue siendo la autoridad de cantidades, costos y cotizacion.
- No se agrega un campo de ultimo costo al producto.
- Ninguna base de produccion se migra antes de cerrar funciones, UI, pruebas y esquema.
- Una base normal de `main` version 8 crea un backup y migra a version 12.
- Productos, stock, ventas y movimientos historicos permanecen disponibles.
- Un fallo de backup o migracion no deja una base parcial.
- El procedimiento de rollback esta probado con el backup v8.
- La version publicada es `v0.2.0`.
