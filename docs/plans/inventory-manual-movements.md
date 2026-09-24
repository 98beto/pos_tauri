# Registro manual de movimientos de inventario

**Estado:** implementado y verificado, con idempotencia persistente, el 23 de septiembre de 2026.

## Objetivo

Permitir que un usuario autenticado registre manualmente:

- Entradas por compra.
- Entradas por ajuste.
- Salidas por ajuste.

El primer alcance solo solicita producto y cantidad. No incluye proveedor, costo de compra, factura ni notas. Las salidas nunca pueden dejar el stock en negativo.

## Implementacion resultante

- `src-tauri/src/services/inventory.rs` contiene la operacion transaccional `create`, que valida la cantidad textual, modifica `products.stock` e inserta el movimiento.
- `create_inventory_movement` expone la operacion como comando Tauri autenticado.
- La frontera IPC conserva `type` en TypeScript, lo mapea a `movement_type` en Rust mediante Serde y envia `quantity` como texto.
- `007` clasifica cada movimiento con `origin`: `sale`, `initial_stock` o `manual`, y aplica una matriz exhaustiva entre origen, motivo, tipo, venta, token y huella. Cada movimiento manual usa un `operation_token`; `008` garantiza su unicidad. Los movimientos de venta y de stock inicial no usan este token.
- Un retry con el mismo token e intencion devuelve el movimiento original. El mismo token con otra intencion devuelve `INVENTORY_OPERATION_TOKEN_REUSED` sin cambiar el stock.
- `InventoryView` persiste antes del IPC una intencion versionada por usuario, hidrata y abre el formulario al recuperarla, comparte solicitudes en vuelo por usuario/token y la elimina mediante compare-and-clear verificable al confirmar el resultado.
- Si el backend confirma el movimiento pero falla la limpieza local, la interfaz conserva la recuperacion y ofrece reintentar solo la limpieza. El descarte tampoco oculta estado hasta verificar la eliminacion.
- Las pruebas cubren las tres operaciones, idempotencia, limites textuales, rollback, autenticacion, persistencia local, recuperacion y estados asincronos del formulario.

## Backend Rust

1. Quitar `#[cfg(test)]` de `CreateInventoryMovementInput` y de `services::inventory::create`.
2. Mantener en Rust todas las validaciones:
   - El producto debe existir.
   - La cantidad llega como texto y debe ser un entero decimal entre `1` y `JS_MAX_SAFE_INTEGER`, sin conversion de punto flotante. El error usa `field: quantity`.
   - Solo se permiten `compra/entrada`, `ajuste/entrada` y `ajuste/salida`.
   - Los movimientos de venta solo pueden originarse en el servicio de ventas.
   - Una salida de ajuste no puede superar el stock actual.
   - Una entrada no puede desbordar el limite seguro de stock.
3. Mantener la consulta/registro idempotente, la actualizacion de stock y la insercion del movimiento dentro de una unica transaccion `IMMEDIATE`, de modo que conexiones concurrentes serialicen la comprobacion del token.
4. Agregar `create_inventory_movement` en `src-tauri/src/commands/inventory.rs` mediante `with_authenticated_connection`.
5. Registrar el comando en `tauri::generate_handler!` dentro de `src-tauri/src/lib.rs`.
6. Devolver `AppError` controlados sin exponer errores internos de SQLite.
7. Validar `operation_token` con longitud maxima de 128 caracteres y guardar una huella canonica versionada de `product_id`, `reason`, `type` y la cantidad ya convertida.

## Frontera React-Tauri

1. Agregar `CreateInventoryMovementInput` en `src/types/inventory_movement.ts`.
2. Agregar `createInventoryMovement(input)` en `src/api/inventory.ts`.
3. Invocar `create_inventory_movement` con `{ input }` y devolver el movimiento creado.
4. Mantener en el frontend solo la captura de valores y la presentacion de errores; Rust decide si la cantidad y el movimiento son validos y calcula el nuevo stock.
5. Generar `operation_token` con `crypto.randomUUID()` y persistir la intencion antes de invocar el comando.

## Interfaz de inventario

1. Pasar `onNotice` desde `AppShell` hacia `InventoryView` para mostrar confirmaciones globales.
2. Agregar el boton `Registrar movimiento` en la cabecera del historial.
3. Abrir un `Modal` con estos campos:
   - Producto existente.
   - Operacion: `Compra`, `Ajuste de entrada` o `Ajuste de salida`.
   - Cantidad entera positiva.
4. Traducir la operacion elegida al contrato del backend:
   - Compra: `{ reason: "compra", type: "entrada" }`.
   - Ajuste de entrada: `{ reason: "ajuste", type: "entrada" }`.
   - Ajuste de salida: `{ reason: "ajuste", type: "salida" }`.
5. Mostrar el stock actual del producto seleccionado como contexto. Este valor es informativo; el backend vuelve a comprobarlo al guardar.
6. Deshabilitar cierre y controles mientras se guarda para evitar envios duplicados.
7. Ante `SESSION_REQUIRED`, cerrar el flujo mediante `onSessionRequired`.
8. Ante otros errores, conservar el formulario abierto y mostrar el mensaje controlado del backend.
   - Asociar los errores `field: quantity` al campo y enfocarlo.
   - Conservar las respuestas inciertas para reintento con el mismo token.
   - Ante un error definitivo, ofrecer descarte confirmado y recarga.
9. Al completar la operacion:
   - Cerrar y limpiar el formulario.
   - Mostrar una notificacion de exito.
   - Recargar estadisticas e historial conservando los filtros actuales.
   - Si falla solamente la recarga, informar que el movimiento se guardo y permitir reintentar la consulta.

## Seleccion de producto

Usar la consulta existente de productos en lugar de duplicar datos. Para el primer alcance puede cargarse `listProducts()` al abrir el formulario y mostrarse un selector con nombre, SKU y stock. Si el catalogo crece y el selector deja de ser practico, sustituirlo posteriormente por una busqueda remota sin cambiar el comando de creacion.

## Pruebas Rust

- Compra incrementa stock y crea un movimiento `compra/entrada`.
- Ajuste de entrada incrementa stock y crea un movimiento `ajuste/entrada`.
- Ajuste de salida reduce stock y crea un movimiento `ajuste/salida`.
- Una salida mayor al stock devuelve `INSUFFICIENT_STOCK` sin modificar stock ni insertar movimiento.
- Producto inexistente, cantidad invalida y combinacion invalida no crean movimientos.
- Un desbordamiento de stock revierte toda la operacion.
- El comando exige una sesion autenticada.
- La primera aplicacion cambia el stock una vez; un retry identico devuelve el mismo movimiento y una intencion distinta con el mismo token se rechaza.
- Un rollback no consume el token y permite reutilizarlo correctamente.
- Cantidades textuales vacias, decimales, con signo o fuera de limites se rechazan con `field: quantity`.
- Dos conexiones concurrentes con el mismo token e intencion aplican un solo cambio y ambas recuperan el movimiento; `02` y `2` generan la misma huella canonica.
- Los `CHECK` de `origin` rechazan directamente toda combinacion fuera de la matriz y el indice unico rechaza tokens manuales duplicados.

## Pruebas frontend

- El adaptador envia `create_inventory_movement` con la forma exacta del input.
- El formulario abre, carga productos y envia cada una de las tres operaciones permitidas.
- Los controles quedan bloqueados durante el guardado.
- Un error conserva el formulario y muestra un mensaje seguro.
- Una sesion expirada llama a `onSessionRequired`.
- Un guardado exitoso cierra el modal, emite la notificacion y recarga la vista sin perder filtros.
- Una recarga fallida despues del guardado no presenta la operacion como fallida.
- La intencion se persiste antes del IPC, se aisla por usuario y solo se elimina si coincide el token.
- Una respuesta perdida seguida de retry o remontaje usa el mismo token y no duplica el movimiento.
- Los errores de cantidad marcan y enfocan el campo; los errores definitivos permiten descartar con confirmacion.
- El retry visible hidrata producto, operacion y cantidad, abre el modal y carga productos mientras resuelve el IPC.
- Un desmontaje/remontaje con una peticion activa se une a la misma promesa, y los fallos de limpieza conservan modal/banner hasta confirmacion verificable.

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

1. Registrar una compra y confirmar que suben el stock, las entradas historicas y la existencia actual.
2. Registrar un ajuste de entrada y confirmar los mismos cambios con motivo `Ajuste`.
3. Registrar un ajuste de salida valido y confirmar que bajan el stock y la existencia actual, y suben las salidas historicas.
4. Intentar retirar mas unidades que las disponibles y confirmar que no cambia el historial ni el stock.
5. Cambiar los filtros, registrar un movimiento y confirmar que la recarga conserva esos filtros.
6. Reiniciar la aplicacion y confirmar que los movimientos y existencias persisten.
