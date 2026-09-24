# Plan de implementacion del POS

## Objetivo

Integrar el diseno y las funcionalidades de `demo.html` en la aplicacion React + Tauri, utilizando los servicios SQLite existentes.

Principios del proyecto:

- Rust concentra las reglas de negocio, validaciones, busquedas, filtros, calculos, carrito y transacciones.
- React captura entradas, invoca comandos de Tauri y renderiza las respuestas.
- El estado local de React se limita a asuntos visuales, como la vista activa, campos, modales y estados de carga.
- Tailwind CSS v4 reemplaza el CSS manual.
- La marca muestra un icono de carrito junto al texto `POS`.
- No se implementa la opcion "Guardar en espera".
- Los errores internos de SQLite no se presentan directamente al usuario.

## Etapa 1: Preparar Tailwind CSS

- Instalar `tailwindcss` y `@tailwindcss/vite`.
- Registrar el plugin de Tailwind en `vite.config.ts`.
- Reemplazar los estilos iniciales de Vite por la importacion de Tailwind y los estilos globales minimos.
- Definir los colores, tipografias, sombras y radios visuales de `demo.html`.
- Mantener como referencia las dimensiones principales del demo:
  - Sidebar de `244px` en escritorio.
  - Barra superior de `88px`.
  - Panel de cobro de `400px`.
  - Formulario de acceso con ancho maximo aproximado de `480px`.
- Adaptar las vistas para resoluciones menores sin romper el flujo de operacion.
- Ajustar el tamano inicial de la ventana de Tauri para acomodar el punto de venta.

### Resultado esperado

Tailwind queda configurado y la aplicacion cuenta con una base visual equivalente al demo.

## Etapa 2: Definir la frontera entre React y Rust

Rust sera responsable de:

- Validar todos los datos enviados por formularios.
- Normalizar correos, nombres, SKU y criterios de busqueda.
- Verificar duplicados.
- Consultar y cruzar datos de SQLite.
- Aplicar filtros y busquedas.
- Calcular estadisticas, totales, subtotales y cambio.
- Verificar existencias.
- Administrar el carrito de la sesion.
- Crear ventas y movimientos de inventario.
- Traducir errores internos a errores de aplicacion.

React sera responsable de:

- Mostrar los datos devueltos por Rust.
- Capturar valores de formularios antes de enviarlos.
- Controlar la navegacion visual.
- Abrir y cerrar modales.
- Mostrar estados de carga, errores y notificaciones.
- Aplicar formato visual de moneda y fecha cuando corresponda.

### Resultado esperado

No quedan reglas comerciales ni calculos de dominio duplicados en el frontend.

## Etapa 3: Preparar autenticacion y aprovisionamiento

### Usuario de desarrollo

- Crear automaticamente un usuario de prueba solo en compilaciones de desarrollo.
- Proteger esta inicializacion con `cfg(debug_assertions)`.
- Crear el usuario solamente cuando la tabla `users` este vacia.
- Generar la contrasena con el mismo hash Argon2 usado por el servicio de autenticacion.
- Usar temporalmente las siguientes credenciales:
  - Correo: `admin@pos.local`
  - Contrasena: `PosDemo123!`
- Mostrar estas credenciales unicamente durante desarrollo.
- Verificar que el usuario de prueba no se incluya en compilaciones release.

### Usuario de produccion

- No incluir credenciales compartidas ni contrasenas fijas dentro del paquete final.
- Utilizar `initialize_owner` para configurar al propietario cuando la instalacion no tenga usuarios.
- Mostrar el formulario de configuracion solo en el primer inicio.
- Despues de crear al propietario, mostrar exclusivamente el login.
- Si posteriormente se decide aprovisionar externamente cada instalacion, sustituir el formulario inicial por una herramienta administrativa que use el mismo servicio de Rust.

### Sesion

- Cambiar la sesion actual de un simple `bool` a una sesion que conserve el usuario autenticado.
- Agregar un comando para consultar el usuario actual.
- Mantener `login`, `logout`, `owner_exists` e `is_authenticated`.
- Limpiar el carrito y la sesion al cerrar sesion.
- Proteger todos los comandos operativos con autenticacion.

### Resultado esperado

El desarrollo tiene acceso inmediato con una cuenta segura de prueba y produccion permite crear un propietario sin credenciales incluidas en el ejecutable.

## Etapa 4: Implementar la pantalla de acceso

- Crear una vista centrada basada en el lenguaje visual del demo.
- Mostrar el icono de carrito junto al texto `POS`.
- Agregar campo de correo y campo de contrasena.
- Permitir enviar el formulario con Enter.
- Deshabilitar campos y boton durante la solicitud.
- Mostrar mensajes de error devueltos por Rust.
- Entrar al punto de venta despues de autenticar correctamente.
- Mostrar la configuracion inicial del propietario solamente cuando `owner_exists` devuelva `false` y no sea una compilacion de desarrollo con usuario sembrado.

### Resultado esperado

El acceso funciona mediante el servicio de autenticacion conectado a SQLite.

## Etapa 5: Administrar el carrito en Rust

Crear un carrito de sesion administrado por el backend de Tauri.

Comandos previstos:

- `get_cart`
- `add_cart_item`
- `increment_cart_item`
- `decrement_cart_item`
- `remove_cart_item`
- `clear_cart`
- `checkout_cart`

Rust sera responsable de:

- Verificar que el producto exista.
- Verificar stock disponible.
- Controlar cantidades positivas y limites de inventario.
- Calcular cantidad de articulos, subtotal y total.
- Calcular el cambio para pagos en efectivo.
- Validar que el efectivo sea suficiente.
- Evitar cobros duplicados.
- Crear venta, detalles y movimientos de inventario en una sola transaccion.
- Actualizar el stock de forma atomica.
- Vaciar el carrito solamente despues de una venta exitosa.
- Mantener el carrito intacto cuando ocurra un error.

El frontend recibira un DTO con las lineas, cantidades, disponibilidad y totales ya calculados.

### Resultado esperado

El frontend no decide precios, cantidades validas, disponibilidad ni resultados del cobro.

## Etapa 6: Crear consultas orientadas a las vistas

Agregar servicios de consulta en Rust que entreguen la informacion necesaria para cada pantalla.

### Productos

- Buscar por nombre o SKU.
- Filtrar por marca y categoria.
- Resolver nombres de marca y categoria.
- Calcular cantidad de productos.
- Calcular unidades en inventario.
- Calcular cantidad de marcas.
- Calcular valor total del inventario.
- Buscar productos disponibles para el selector del carrito.
- Restar del disponible las cantidades agregadas al carrito.

### Inventario

- Buscar por producto o SKU.
- Filtrar por motivo y tipo de movimiento.
- Resolver datos del producto.
- Calcular entradas y salidas acumuladas.
- Calcular stock actual y productos agotados.
- Generar referencias de tickets para movimientos de venta.

### Ventas

- Buscar por numero de ticket.
- Filtrar por metodo de pago.
- Calcular total vendido.
- Calcular ticket promedio.
- Calcular cantidad de articulos vendidos.
- Retornar el resumen de cada venta.

### Resultado esperado

React recibe filas y estadisticas listas para representar, sin filtrar ni cruzar colecciones localmente.

## Etapa 7: Completar la consulta de detalles de venta

El servicio actual `list_sales` solo devuelve los encabezados de venta. Para completar el demo se debe:

- Agregar una consulta de venta con sus detalles.
- Incluir producto, cantidad, precio unitario y subtotal.
- Incluir la cantidad total de articulos.
- Resolver el nombre del producto desde Rust.
- Proteger la consulta con autenticacion.
- Permitir consultar ventas creadas en sesiones anteriores.

### Resultado esperado

El modal de detalle funciona despues de reiniciar la aplicacion y no depende de datos temporales del frontend.

## Etapa 8: Implementar el layout principal

Portar el diseno de `demo.html` a componentes React con Tailwind:

- Sidebar.
- Marca con carrito y texto `POS`.
- Navegacion para Punto de venta, Productos, Inventario y Ventas.
- Informacion del propietario autenticado.
- Accion para cerrar sesion.
- Barra superior con titulo, descripcion, estado y fecha.
- Modales reutilizables.
- Estados vacios.
- Notificaciones.
- Tablas y paneles adaptables al espacio disponible.

### Resultado esperado

La estructura visual coincide con el demo y utiliza datos de sesion reales.

## Etapa 9: Implementar Punto de Venta

- Mostrar el carrito devuelto por Rust.
- Abrir un selector de productos.
- Enviar busquedas y filtros de categoria a Rust.
- Agregar, incrementar, reducir y eliminar mediante comandos.
- Permitir seleccionar efectivo, tarjeta o transferencia.
- Capturar efectivo recibido y solicitar el calculo a Rust.
- Bloquear acciones mientras se procesa el cobro.
- Ejecutar el cobro transaccional.
- Actualizar productos, ventas e inventario despues de cobrar.
- Agregar el atajo `F2` para abrir el selector cuando no interfiera con otro campo o modal.
- No mostrar el boton "Guardar en espera".

### Resultado esperado

El flujo de venta queda conectado a SQLite y conserva integridad ante errores.

## Etapa 10: Implementar Productos y Catalogos

- Mostrar la tabla de productos.
- Enviar busqueda y filtros a Rust.
- Mostrar estadisticas generadas en Rust.
- Crear productos.
- Editar productos sin alterar directamente su stock.
- Mostrar stock inicial solamente durante la creacion.
- Convertir y validar valores monetarios en Rust.
- Administrar marcas y categorias con los servicios existentes.
- Mostrar errores de SKU, marca o categoria duplicados devueltos por Rust.
- Actualizar selectores y tablas despues de cada operacion exitosa.

### Resultado esperado

El catalogo completo opera sobre SQLite y no replica validaciones en React.

## Etapa 11: Implementar Inventario

- Mostrar movimientos reales de inventario.
- Enviar busquedas y filtros a Rust.
- Mostrar estadisticas generadas en Rust.
- Mostrar referencias de ticket en movimientos de venta.
- Diferenciar visualmente entradas, salidas, compras, ajustes y ventas.
- No agregar formularios de movimientos que no formen parte del alcance actual del demo.

### Resultado esperado

El historial refleja las existencias y operaciones persistidas en SQLite.

## Etapa 12: Implementar Ventas

- Mostrar la tabla de tickets.
- Enviar busqueda y filtro de metodo de pago a Rust.
- Mostrar estadisticas generadas en Rust.
- Abrir el detalle de una venta.
- Mostrar metodo de pago, articulos, total y fecha.
- Actualizar la vista despues de cada cobro exitoso.

### Resultado esperado

El historial de ventas es persistente y consultable desde la aplicacion.

## Etapa 13: Estandarizar errores

- Definir errores serializables en Rust.
- Separar errores de validacion, conflicto, autenticacion, datos inexistentes y base de datos.
- No exponer mensajes internos de SQLite.
- Manejar como minimo:
  - Credenciales incorrectas.
  - Sesion no autorizada.
  - Stock insuficiente.
  - SKU duplicado.
  - Marca o categoria duplicada.
  - Producto inexistente.
  - Efectivo insuficiente.
  - Base de datos sin propietario.
- Hacer que React solo determine la presentacion visual del error recibido.

### Resultado esperado

Los errores son consistentes y comprensibles en todas las vistas.

## Etapa 14: Pruebas y verificacion

### Rust

Agregar o completar pruebas para:

- Creacion del usuario de desarrollo solo en debug.
- Configuracion del propietario.
- Login correcto e incorrecto.
- Proteccion de comandos.
- Cierre de sesion.
- Operaciones del carrito.
- Limites de stock.
- Calculo de totales y cambio.
- Cobro con cada metodo de pago.
- Rollback ante errores de venta.
- Busquedas, filtros y estadisticas.
- Detalle de ventas historicas.
- Limpieza del carrito al cerrar sesion.

### Frontend

Verificar:

- Login y configuracion inicial.
- Navegacion.
- Formularios y modales.
- Estados de carga y error.
- Navegacion con teclado.
- Diseno en diferentes dimensiones.
- Ausencia de errores en consola.

### Comandos de verificacion

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
pnpm build
pnpm tauri dev
```

## Criterios de finalizacion

- El diseno principal conserva el lenguaje visual de `demo.html`.
- La marca muestra un carrito junto al texto `POS`.
- La interfaz utiliza Tailwind CSS.
- El login utiliza el servicio de autenticacion y SQLite.
- El usuario de prueba solo existe en desarrollo.
- La compilacion release no contiene credenciales predeterminadas.
- React no contiene validaciones ni calculos comerciales.
- Productos, inventario y ventas usan datos reales.
- El carrito es administrado por Rust.
- Una venta actualiza SQLite de manera atomica.
- El historial y sus detalles sobreviven al reinicio.
- La opcion "Guardar en espera" no aparece.
- El frontend y el backend compilan correctamente.
