# Registro de facturas de proveedores desde XML

**Estado:** implementación completa, revisiones independientes sin bloqueantes y gates automatizados aprobados en Linux. Arranque Tauri aislado y smoke visual Chromium completados; el recorrido visual usa IPC simulado. La interacción manual con los selectores/visor nativos y la verificación en Windows siguen pendientes, como se detalla en los resultados de ejecución.

**Base del plan:** SQLite 15, con baja lógica y sesiones de caja; entrega actual: SQLite 16.

**Objetivo:** importar, revisar, guardar y consultar facturas de compra sin capturar
manualmente sus datos principales y sin aplicar entradas automáticas al inventario.

## 1. Decisiones acordadas

- Los proveedores entregan XML y PDF de sus facturas.
- El XML CFDI será la fuente de datos; el PDF será un adjunto opcional.
- Se añadirá una sección **Compras** para registrar y consultar facturas.
- Los tres filtros principales serán proveedor, número de factura y fecha de factura.
- El listado tendrá tabla y tarjetas, con tabla inicial y preferencia local recordada,
  igual que las otras vistas de la app.
- Los conceptos se conservarán tal como aparecen en el documento, sólo para consulta.
- **Ver detalle** mostrará los conceptos en una tabla con los datos originales del
  proveedor; no se generará una representación visual adicional de la factura.
- La captura de productos y de compras de inventario seguirá siendo manual.
- Importar una factura no crea productos, presentaciones, entradas, retiros ni pagos.
- El funcionamiento inicial será local, sin correo, OCR, IA ni servidor externo.

La propuesta anterior de relacionar conceptos con el catálogo y registrar existencias
queda aplazada. Tampoco hace falta decidir un precio de compra con o sin impuestos:
esta función conserva por separado los importes originales del comprobante.

## 2. Alcance de la primera versión

### Incluido

1. Importar un XML CFDI 4.0 timbrado de tipo ingreso (`I`).
2. Extraer emisor, receptor, identificación, fechas, moneda, importes y conceptos.
3. Revisar los datos antes de confirmar el registro.
4. Guardar el XML original y, opcionalmente, un PDF elegido por el usuario.
5. Consultar facturas con filtros combinables, paginación y alternancia tabla/tarjetas.
6. Ver detalle y exportar el XML/PDF; abrir el PDF mediante el visor del sistema.
7. Detectar duplicados por UUID y recuperar un registro ya confirmado tras reintentar.
8. Adjuntar un PDF posteriormente si la factura todavía no tiene uno.
9. Conservar los archivos dentro del respaldo y restauración de la base de datos.

### Fuera de alcance

- Crear o modificar productos, códigos, unidades o presentaciones desde la factura.
- Vincular conceptos con el catálogo o registrar movimientos de inventario.
- Pagar facturas, retirar dinero de caja, cuentas por pagar o conciliación bancaria.
- Extracción de datos de PDF, fotos, escaneos, tickets o remisiones sin XML.
- Lectura de correo, descarga automática del SAT o almacenamiento de credenciales SAT.
- Verificación de sellos, vigencia, cancelación o tratamiento fiscal ante el SAT.
- Notas de crédito (`E`), complementos de pago (`P`), traslado, nómina y CFDI 3.3.
- Reemplazar archivos originales, editar datos fiscales importados o eliminar facturas.
- Conversiones de moneda o sumas de importes de distintas monedas.
- Generar una factura visual propia o reconstruir el diseño impreso de cada proveedor.

Los documentos no admitidos deben identificarse con un mensaje específico. No
interpretar un complemento de pagos como una nueva factura de mercancía.

## 3. Situación inicial y puntos de integración

La tabla siguiente conserva el contexto original del plan, no el estado actual. Compras, parser, comandos nativos, adjuntos y migración 016 están implementados; falta la verificación final.

| Área | Estado y cambio necesario |
| --- | --- |
| Navegación | `src/layout/navigation.tsx` y `AppShell.tsx` tienen POS, Caja, Productos, Inventario y Ventas; añadir Compras. |
| Compras existentes | `models/inventory_movement.rs` y `services/inventory.rs` capturan movimientos de un producto con proveedor, referencia y precio. Son operaciones distintas y permanecen manuales. |
| Catálogo | Incluye archivo/restauración y presentaciones; este importador no necesita una relación con sus IDs. |
| Caja | Los cobros y operaciones de efectivo tienen sus propios contratos; no invocarlos al importar. |
| SQLite | Última migración registrada: 015; crear la siguiente sin reescribir 001–015. |
| Archivos | No hay un flujo de adjuntos ni parser XML. Incorporar selección/guardado local y procesamiento Rust acotado. |
| Respaldos | El migrador valida esquemas, datos retenidos y recuperación durable; ampliar esas garantías para las tablas y BLOB nuevos. |

## 4. Experiencia de usuario

### 4.1 Listado de Compras

Encabezado con **Importar factura** y estos controles:

| Filtro | Regla |
| --- | --- |
| Proveedor | Buscar por nombre o RFC y seleccionar uno; la identidad es el RFC normalizado. |
| N.º de factura | Coincidencia parcial sobre serie, folio, serie-folio o UUID, sin distinguir mayúsculas. |
| Fecha de factura | Desde/hasta inclusivos sobre la fecha de emisión del XML. |

- Combinar los tres filtros y ofrecer **Limpiar filtros**.
- Ordenar por fecha de emisión descendente y desempatar por ID descendente.
- Mostrar fecha, proveedor, número de factura, total con moneda y **Ver detalle**.
- Tabla como presentación inicial y selector **Lista / Tarjetas**, usando el mismo
  control que las vistas existentes; **Lista** corresponde a la tabla de facturas.
- Reutilizar `ViewToggle` y `useViewPreference` con una clave propia de Compras.
- Tabla y tarjetas deben mostrar los mismos datos principales y acciones.
- Cambiar de presentación no debe recargar datos ni alterar filtros, orden o página.
- Conservar la preferencia al salir y volver, tolerando almacenamiento no disponible
  o un valor guardado inválido con el mismo comportamiento de las otras vistas.
- Usar serie y folio cuando existan; si faltan ambos, mostrar el UUID.
- Paginación backend de 25 filas, con total del conjunto filtrado.
- Reiniciar a página 1 al cambiar filtros; conservar filtros y página al volver del detalle.
- Los totales mostrados pertenecen a cada documento; no agregar monedas diferentes.
- Distinguir carga, error, lista vacía y filtros sin resultados.

### 4.2 Importación y revisión

1. Seleccionar el XML y, opcionalmente, su PDF.
2. Extraer y validar en Rust; todavía no guardar una factura.
3. Mostrar proveedor/RFC, receptor, fecha, serie/folio, UUID, moneda y desglose.
4. Mostrar la misma tabla de conceptos documentales prevista para el detalle, con
   cantidades, unidades, precios, descuentos e impuestos originales. No ofrecer
   selectores de producto o conversiones a inventario.
5. Indicar que **registrar el documento no registra mercancía recibida ni un pago**.
6. Confirmar con **Registrar factura** o cancelar sin crear filas/adjuntos persistentes.
7. Tras confirmar, abrir el detalle guardado y actualizar el listado.

La revisión es temporal: no crear un estado persistente de recepción ni un borrador
de inventario. La factura guardada significa únicamente **documento registrado**.

### 4.3 Detalle con tabla de conceptos

Al pulsar **Ver detalle**, presentar un encabezado compacto con proveedor/RFC,
receptor, número de factura, fecha, UUID, moneda y fecha de registro en la app.
Debajo se mostrará una tabla de los productos o conceptos del proveedor.

Columnas documentales:

| Columna | Fuente y comportamiento |
| --- | --- |
| Código del proveedor | `NoIdentificacion` cuando exista; no confundirlo con un SKU de la POS. |
| Descripción | Texto original del concepto, sin sustituirlo por el nombre del catálogo. |
| Cantidad | Decimal del XML, conservando su precisión. |
| Unidad | Descripción original si existe y acceso a `ClaveUnidad`; sin conversiones. |
| Precio unitario | `ValorUnitario` original, sin sustituirlo por un precio de venta o compra de la POS. |
| Descuento | Valor informado por el proveedor, distinguiendo ausencia de cero. |
| Importe | `Importe` del XML; no reemplazarlo por un cálculo redondeado del frontend. |

- Conservar el orden de las líneas del documento.
- Hacer visibles las columnas opcionales cuando al menos un concepto tenga ese
  dato; en las filas donde falte, mostrar **No informado**. Las columnas básicas de
  descripción, cantidad, unidad, precio e importe permanecen visibles.
- Permitir consultar por línea `ClaveProdServ`, `ClaveUnidad` y el desglose de
  impuestos trasladados/retenidos extraídos. Usar una expansión o detalle accesible
  por fila si el desglose es amplio, sin saturar la tabla principal.
- Mostrar al pie o después de la tabla subtotal, descuentos, impuestos y total del
  comprobante, con su moneda, conservando los valores originales.
- La tabla es de consulta: sin edición, vínculo con catálogo ni acciones de inventario.
- En ventanas estrechas, usar una región de tabla desplazable y accesible; conservar
  textos completos mediante ajuste de línea o consulta de detalle, sin cortar datos.
- Para documentos extensos, paginar o renderizar por bloques los conceptos sin
  cambiar su orden ni omitirlos silenciosamente.

**Formato del proveedor:** se conservan su contenido y atributos estándar presentes
en el XML, no los colores, encabezados personalizados, anchos ni distribución del
PDF. El XML no describe necesariamente ese diseño. Las addendas se conservan dentro
del archivo original, pero no se promete interpretar sus atributos particulares.

El detalle funcional será encabezado, tabla y desglose, sin un diseño nuevo que
imite una factura impresa. **Abrir PDF** permite consultar el formato exacto del
documento adjunto del proveedor.

### 4.4 Archivos originales

- Exportar el XML original y el PDF adjunto a una ubicación elegida por el usuario.
- Si falta PDF, permitir adjuntarlo desde el detalle, sin editar el XML.
- Abrir el PDF con el visor del sistema mediante una copia temporal controlada;
  no cargar contenido PDF dentro del DOM de la app en esta primera versión.
- Identificar el PDF como adjuntado por el usuario: el nombre del archivo no demuestra
  que corresponda al UUID del XML.

## 5. Extracción y representación de datos

### 5.1 Lectura del XML

Seleccionar un parser Rust con soporte de namespaces. Resolver los elementos por
namespace y nombre local, no por prefijos literales como `cfdi` o `tfd`. No descargar
esquemas, catálogos o recursos remotos durante la lectura.

Validaciones mínimas:

- Archivo legible y XML bien formado; límite inicial de 5 MiB.
- Sin DTD ni expansión de entidades externas; límites de profundidad y conceptos
  para no agotar recursos. Propuesta inicial: 64 niveles y 10,000 conceptos.
- `Comprobante` del namespace CFDI 4.0, versión `4.0`, tipo `I`.
- Un único timbre fiscal con UUID válido, normalizado para deduplicación.
- Emisor/receptor y RFC; fecha de emisión; moneda; subtotal/total y conceptos requeridos.
- Decimales no negativos con sintaxis válida y límites explícitos de precisión/tamaño.
- Serie, folio, identificador del concepto, unidad descriptiva y descuento son
  opcionales cuando el estándar lo permita; conservar ausencia frente a cero.

La extensión o el MIME declarados no sustituyen la comprobación de contenido.
Admitir prefijos variables, caracteres acentuados y addendas sin inferir campos
de negocio desde formatos específicos de proveedor.

No denominar este resultado **validado por SAT**. Las comprobaciones aritméticas
derivadas deben distinguirse de la extracción: conservar los valores originales y
mostrar discrepancias cuando puedan demostrarse, sin corregirlos silenciosamente
ni rechazar facturas por una regla de redondeo inventada.

### 5.2 Campos a extraer

- Encabezado: versión, tipo, serie, folio, UUID, fecha de emisión y timbrado si existe.
- Emisor y receptor: RFC y nombre, conservando el snapshot del documento.
- Moneda y tipo de cambio cuando venga informado, sin convertir a pesos.
- Subtotal, descuento, impuestos trasladados/retenidos y total, distinguiendo ausencia.
- Conceptos en su orden: descripción, `NoIdentificacion`, `ClaveProdServ`, `ClaveUnidad`,
  unidad descriptiva, cantidad, valor unitario, importe, descuento y desglose fiscal.

`ClaveProdServ` no es el SKU de la POS. Las claves y cantidades quedan como datos
documentales: ninguna de ellas genera operaciones contra el catálogo.

### 5.3 Importes, cantidades y fechas

- No usar `float`, `parseFloat` o `Number` como representación de valores fiscales.
- Conservar los decimales originales como texto validado; usar aritmética decimal
  exacta si se calcula una comprobación auxiliar. En IPC también viajan como texto.
- El formato visual debe respetar la precisión y moneda del documento, sin pasarlos
  al formateador actual de centavos MXN ni redondear cantidades a piezas enteras.
- Conservar `Fecha` original y una fecha calendario de emisión para filtrar. No
  convertir una fecha sin zona horaria a UTC y desplazarla al día anterior/siguiente.
- La fecha de importación y el usuario registrador son auditoría separada.

## 6. Modelo de persistencia

### `purchase_invoices`

- ID, UUID único normalizado y hash SHA-256 del XML original.
- Serie/folio opcionales y clave normalizada de búsqueda del número.
- RFC/nombre del emisor y receptor como snapshots; claves de búsqueda normalizadas.
- Fecha original, fecha calendario indexada, moneda y tipo de cambio original.
- Importes decimales documentales y desglose fiscal estructurado.
- Usuario registrador, snapshot de su nombre y fecha de registro.

### `purchase_invoice_lines`

- FK a factura y posición única dentro de ésta.
- Campos documentales de conceptos y decimales como texto validado.
- Desglose fiscal conservado en una estructura definida, sin relaciones a productos,
  presentaciones ni movimientos de inventario.

### `purchase_invoice_attachments`

- FK a factura, clase `xml` o `pdf`, nombre original saneado, MIME, tamaño y SHA-256.
- Contenido original como BLOB; un XML obligatorio y como máximo un PDF por factura.
- PDF opcional de hasta 15 MiB, con comprobación de firma/formato básica.
- Límites comprobados antes de cargar/copiar contenido completo en memoria.

Guardar los archivos en SQLite evita referencias a rutas externas que pueden
desaparecer y permite respaldar metadatos y documentos conjuntamente. El listado
no debe cargar BLOB ni conceptos; se consultan al abrir el detalle o exportar.

No hace falta un CRUD de proveedores en esta versión. El selector se obtiene de
facturas registradas, agrupadas por RFC, con un nombre representativo y búsqueda
sobre los nombres originales. Agrupar no debe reescribir los snapshots anteriores.

## 7. Servicios, contratos e idempotencia

Módulos implementados: modelos/servicios/comandos `purchase_invoices` en Rust;
`src/types/purchase_invoice.ts`, `src/api/purchase-invoices.ts` y feature de Compras.

Operaciones semánticas:

| Operación | Responsabilidad |
| --- | --- |
| Revisar XML | Seleccionar/leer archivo, extraer y devolver vista previa con hash. |
| Registrar factura | Revalidar los bytes originales revisados y guardar todo atómicamente. |
| Listar facturas | Aplicar filtros, orden y paginación sin cargar adjuntos. |
| Buscar proveedores | Devolver RFC/nombre para el filtro, con búsqueda normalizada. |
| Obtener detalle | Devolver encabezado, conceptos y metadatos de archivos. |
| Adjuntar PDF | Añadirlo sólo si falta, con operación idempotente. |
| Exportar/abrir archivo | Resolver por ID y clase desde la base, no por una ruta almacenada. |

- Todas las operaciones IPC requieren la autenticación local existente.
- El preview capturado en backend tiene vigencia de 30 minutos y queda invalidado al cerrar sesión mediante la generación de sesión; no se relee la ruta al confirmar.
- No recibir `user_id` del frontend para decidir la autoría.
- Elegir selección/guardado nativos acotados a los archivos del flujo. Consultar la
  documentación vigente de Tauri y del parser antes de fijar dependencias/permisos.
- La vista previa no autoriza a enviar metadatos fiscales modificados desde JS:
  el backend vuelve a extraerlos del XML exacto al confirmar.
- Capturar los bytes para la revisión, o volver a leer y comparar hash. Si el archivo
  cambia después de revisarlo, exigir otra revisión; no confirmar datos distintos.
- Registrar encabezado, conceptos y adjuntos en una transacción corta. Parsear y
  seleccionar archivos fuera de la transacción; rollback completo si falla una escritura.
- Índice único de UUID como última protección frente a doble clic y concurrencia.
- Mismo UUID y mismo XML: devolver la factura existente, sin duplicar documentos.
- Mismo UUID con XML distinto: conservar el original e informar la diferencia;
  no sobrescribirlo automáticamente ni crear otra factura.
- Un PDF ofrecido durante una importación duplicada no se añade silenciosamente;
  permitir hacerlo explícitamente desde el detalle si falta.
- Adjuntar el mismo PDF dos veces recupera el existente; otro hash no reemplaza uno guardado.
- Ante respuesta perdida, reintentar el mismo XML/UUID recupera el registro existente.
- Separar fallo de registro de fallo de recarga del listado. Una factura confirmada
  no vuelve a mostrarse como pendiente de registrar porque falle la recarga.

Los mensajes deben distinguir archivo no admitido, XML ilegible, versión/tipo no
soportado, datos requeridos ausentes, duplicado, contenido diferente y error de guardado.

## 8. Migraciones, respaldo y compatibilidad

- Nueva migración 016 si el esquema sigue en 15 al implementar; reconciliar el número
  si otro cambio agrega una migración antes. No editar migraciones anteriores.
- Añadir tablas/índices/FK sin modificar productos, ventas, movimientos o sesiones de caja.
- Actualizar registro, versión actual, validación canónica y pruebas de integridad.
- Conservar apertura/restauración de fuentes ya admitidas; añadir la fuente v15.
- Ampliar QA con base v15 que tenga ventas, caja abierta/cerrada, movimientos y
  productos archivados; comparar literalmente esos datos antes/después.
- Probar que backup/restauración y reapertura conservan bytes y hashes de adjuntos.
- Mantener compatibilidad de los marcadores durables existentes y documentar el
  destino real; no renombrar archivos de recuperación sin una estrategia compatible.
- Los entrypoints de QA históricos deben seguir funcionando, ampliados al nuevo
  destino; añadir una entrada explícita si sigue el patrón de versiones actual.
- Revisar el guard transversal de referencias: si cambian archivos fijados por hash,
  actualizar pins únicamente tras revisar sus cambios, sin ampliar excepciones generales.

## 9. Bloques de implementación y revisión

### Bloque 0 — Fixtures y contratos

Confirmar esquema real al comenzar, elegir parser y frontera de archivos y preparar
fixtures sintéticos CFDI 4.0 con namespaces, impuestos y decimales. Si hay ejemplos
reales, usarlos sólo anonimizados y fuera de commits cuando contengan datos privados.
No depender de recibirlos para desarrollar el contrato común.

**Resultado:** DTO y errores documentados, límites, dependencias y esquema definidos.

### Bloque 1 — Persistencia y migración (complejo)

Tablas/índices/FK, contratos de listado/detalle y ampliación del migrador/QA. Propietario
de archivos compartidos `db`, migraciones y validadores; otros bloques no los editan
simultáneamente.

**Aceptación:** v15 migra conservando todos los datos previos; no aparecen movimientos
ni operaciones de caja; adjuntos sobreviven respaldo y restauración.

### Bloque 2 — Extracción y registro (complejo)

Parser, revisión, registro atómico, deduplicación, adjuntos y exportación. Registrar
comandos y mantener DTO Rust/TypeScript sincronizados; no reutilizar `create_purchase`
para guardar una factura.

**Aceptación:** variaciones de prefijo/formato XML no requieren plantillas; duplicados
y fallos de escritura no generan registros parciales ni cambian otras entidades.

### Bloque 3 — Interfaz de Compras (rutinario sobre contratos cerrados)

Navegación, listado con tres filtros, paginación y tabla/tarjetas con preferencia
recordada; revisión y detalle con tabla de conceptos originales del proveedor.
Reutilizar los controles de vista existentes y compartir la tabla de conceptos entre
revisión y detalle para evitar diferencias de datos. Seleccionar XML, adjuntar/exportar
archivos y conservar estado al volver. Mostrar claramente el carácter documental del
registro y gestionar loading/error/empty/autenticación.

**Aceptación:** se importa y consulta una factura completa sin capturar sus datos
principales a mano, sin crear productos ni pedir equivalencias de unidades. El listado
alterna tabla/tarjetas sin perder filtros y el detalle conserva códigos, descripciones,
unidades, precios, importes y atributos documentales sin reconstruir un diseño impreso.

### Bloque 4 — Integración, auditoría y documentación

Revisar contratos, límites, importes originales, duplicados, copia de archivos y
preservación de inventario/caja. Actualizar guías de uso, QA y migración; ejecutar
pruebas completas y smoke en perfiles aislados.

Cada bloque requiere revisión independiente. Las correcciones importantes se
vuelven a revisar en una sesión nueva. Si se usa orquestación, paralelizar sólo
archivos sin propiedad compartida y no delegar por duplicado el mismo trabajo.

## 10. Pruebas requeridas

### Parser y documentos

- XML válido de diferentes emisores, prefijos distintos y UTF-8 con acentos.
- Serie/folio y campos opcionales ausentes; fallback al UUID.
- Descuentos, varios impuestos, retenciones, decimales y moneda original.
- XML truncado, namespaces incorrectos, UUID ausente/duplicado, versión/tipo no admitidos.
- DTD, límites de tamaño/profundidad/conceptos y decimales fuera del contrato.
- PDF opcional, formato/tamaño no admitido y exportación de bytes idénticos al original.

### Persistencia y filtros

- Confirmación con todas las líneas/archivos o rollback completo por fallo inyectado.
- Doble envío, concurrencia de UUID y respuesta perdida sin duplicados.
- Igual UUID con contenido diferente conserva el primer documento.
- PDF añadido después y reintentos; rechazo de reemplazos no soportados.
- Filtros combinados proveedor/número/fechas, nombres distintos bajo mismo RFC,
  UUID parcial, fechas inclusivas, orden estable y paginación sin saltos.
- Búsqueda y listado no leen los BLOB.
- Snapshot de productos, presentaciones, ventas, movimientos y caja idéntico antes
  y después de importar, incluso sin caja abierta o con productos archivados.

### Interfaz y regresión

- Cancelar revisión no registra nada; error de extracción conserva un flujo recuperable.
- Doble clic y fallos de recarga no vuelven a registrar una factura confirmada.
- Filtros y página se conservan al abrir/cerrar detalle.
- Tabla inicial y alternancia a tarjetas con datos/acciones equivalentes, sin nuevas
  consultas por cambiar de presentación y sin perder filtros ni página.
- Preferencia de Compras independiente de otras vistas; restauración al navegar y
  fallback correcto ante almacenamiento denegado o valores inválidos.
- Tabla de conceptos idéntica en revisión y detalle: orden, código del proveedor,
  descripción, unidad, precisión, precio, descuento e importe originales.
- Columnas opcionales y ausencias, expansión fiscal por línea y documentos con muchos
  conceptos; no reemplazar datos por los del catálogo aunque haya descripciones parecidas.
- Mostrar decimales, moneda, fecha y ausencias sin conversiones numéricas o de zona indebidas.
- Navegación por teclado, foco, mensajes y layouts estrechos de listado/revisión/detalle.
- Compras manuales, conteos, ventas, archivo de productos y caja siguen funcionando.

### Comandos de cierre

```sh
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --release --manifest-path src-tauri/Cargo.toml
pnpm qa:migration-v8-v13
pnpm qa:migration-v16
```

Los comandos de cierre se ejecutaron correctamente en Linux; las pruebas del backend cubren lectura/exportación sobre archivos temporales reales y el smoke visual cubre el flujo con IPC simulado. Queda pendiente el recorrido interactivo de selectores y visor del sistema en Linux/Windows; no se afirma que ese recorrido manual ni las pruebas de Windows se hayan ejecutado.
Si se añaden dependencias justificadas, actualizar y revisar sus locks; no confundir
esa actualización deliberada con un cambio inesperado producido por una instalación.

## 11. Criterios de finalización

- Se registra una factura a partir de XML y PDF opcional, previa revisión.
- Proveedor, número y fecha permiten encontrarla sin revisar archivos manualmente.
- El listado permite tabla/tarjetas y conserva la preferencia y los filtros.
- Los conceptos se consultan en tabla con los datos originales del proveedor, sin
  vinculación obligatoria al catálogo ni una representación impresa generada por la app.
- Los importes, unidades y documentos originales se conservan fielmente.
- UUID repetido no produce otra factura; reintentar es seguro.
- No cambia ninguna existencia, venta, precio de catálogo o saldo de caja.
- XML y PDF guardados se pueden exportar y quedan cubiertos por respaldo/restauración.
- Nuevas migraciones y regresiones pasan; revisiones independientes sin bloqueantes.
- Se documentan las plataformas y casos efectivamente verificados.

## 12. Referencias

- [SAT: facturas recibidas y representación impresa](https://www.sat.gob.mx/minisitio/Factura/solicita_factura.htm).
- [SAT: guía de llenado CFDI, Anexo 20](http://omawww.sat.gob.mx/tramitesyservicios/Paginas/documentos/Anexo_20_Guia_de_llenado_CFDI.pdf).
- [Plan ya implementado de catálogo y caja](./catalog-layout-cash-sessions.md).
- [Control de calidad](../quality-assurance.md) y [runbook de releases](../releases.md).

Consultar la edición vigente de las referencias y los contratos existentes al
implementar; no tratar la extracción local como una certificación del documento.

## 13. Resultados de ejecución

- Parser, contratos, persistencia/archivos e interfaz revisados independientemente.
  Se corrigieron y volvieron a revisar estructura XML ambigua, límites de comentarios,
  fronteras de búsqueda por número/UUID, refresco de proveedores y autenticación de
  la limpieza posterior al registro. Auditoría integrada sin nuevos bloqueantes.
- `pnpm test`: 379 pruebas frontend, 99 del guard transversal, 12 de versión y 124
  de release aprobadas. Dos pruebas de release omitidas por condiciones de plataforma
  o herramientas opcionales; no se presentan como verificaciones realizadas.
- `pnpm build`, formato y Clippy con `-D warnings`: aprobados.
- Rust debug: 214 pruebas de biblioteca y 5 de integración aprobadas. Release: 215
  de biblioteca y 5 de integración aprobadas. Dos QA se ignoran en el runner ordinario;
  los entrypoints de migración se ejecutaron explícitamente.
- `pnpm qa:migration-v8-v13` y `pnpm qa:migration-v16`: aprobados para las cinco
  fuentes admitidas hasta esquema 16, con backups, reapertura y restauración.
- Migraciones 001–015 y `pnpm-lock.yaml` sin cambios. El cambio deliberado de
  `Cargo.lock` corresponde a las dependencias de parser, diálogos, visor y archivos.
- Arranque `pnpm tauri dev --no-watch` con XDG aislado: Vite listo y binario nativo
  ejecutado, esquema 16, tabla de facturas vacía y sin violaciones de FK. Se detuvo
  mediante timeout controlado; no se accedió al perfil de datos real.
- Chromium sobre la app Vite real, con IPC simulado y fixtures sintéticos: importación,
  revisión, PDF opcional, confirmación, detalle, impuestos por línea, 51 conceptos
  paginados, exportación cancelada, apertura del visor simulada, refresco de proveedor,
  filtros conservados y alternancia a tarjetas sin nuevas consultas del listado.
- Smoke visual a 1280 y 360 px, precisión decimal conservada, desbordamiento horizontal
  acotado a la tabla de conceptos, foco dentro del modal y retorno al disparador al
  cerrar con Escape. Capturas y reporte conservados temporalmente fuera del repositorio.

Las pruebas nativas automatizadas usan archivos temporales reales y selectores
inyectados para cancelación y cambios de sesión; la parte visual simula IPC. No
sustituyen una prueba end-to-end del selector, exportación y visor reales dentro de
la WebView, ni una comprobación en Windows. Éstos son los límites de la evidencia,
no capacidades adicionales afirmadas por la implementación.
