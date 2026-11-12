# Compras documentales — contrato FROZEN v1

Contrato compartido de la entrega implementada. Fuente ejecutable: `src-tauri/src/models/purchase_invoice.rs` y
`src/types/purchase_invoice.ts`; adaptadores: `src/api/purchase-invoices.ts`.
Cambiar nombres, nulabilidad o semántica requiere coordinar ambos consumidores.
Los comandos nativos, diálogos y persistencia documental están implementados en el esquema 16. La verificación final sigue pendiente. No invocar compras de inventario desde este flujo.

## Convenciones de transporte

- Comandos `snake_case`, argumentos externos Tauri `camelCase`: `reviewToken`,
  `invoiceId`. Dentro de DTO y `filters`, claves `snake_case` sin conversiones JS.
- Todos los campos de salida existen; `Option<T>` es `T | null`, no se omite.
  Sólo las propiedades de entrada de filtros son opcionales (ausencia/null = sin filtro).
- IDs positivos dentro del entero seguro JS. Páginas/totales/tamaños son enteros;
  `size_bytes` y contadores se representan como `u32` en Rust. No transportar BLOB,
  rutas, bytes, metadatos fiscales editados ni autoría desde el frontend.
- Decimales **string**, sintaxis `[0-9]{1,18}(\.[0-9]{1,6})?`, no signo,
  exponente, espacios ni separadores de millares. Se preservan ceros y precisión
  lexical (`0010.123456`, `2.000`). Cantidad estrictamente positiva; restantes
  decimales no negativos. Ausencia y `"0"` son distintos.
- No `Number`, `parseFloat`, centavos MXN, conversión a stock, cambio de moneda,
  redondeo ni reconstrucción de importes. No se valida aritmética, sellos ni SAT.
- Fechas CFDI: `YYYY-MM-DDTHH:mm:ss`, calendario gregoriano real, años 0001–9999,
  hora 00–23, minutos/segundos 00–59, sin zona ni fracciones. Se conserva el texto.
  `issued_date` es sus primeros 10 caracteres, nunca una conversión UTC.
- RFC canónico: trim + mayúsculas, 12/13 caracteres con prefijo de 3/4 letras
  (`A-Z`, `Ñ`, `&`), seis dígitos y tres alfanuméricos ASCII. No validación SAT.
  Nombres, descripciones y códigos conservan el texto XML decodificado original;
  el archivo adjunto conserva **bytes** originales, no una serialización del árbol.
- UUID normalizado a mayúsculas `8-4-4-4-12` hex, sin imponer versión RFC4122.

## DTO documentales

`InvoiceParty { rfc, name }`: snapshot de emisor/receptor. Los nombres nunca se
reescriben por una agrupación de proveedores. Búsqueda por nombre usa la clave
Unicode `search_key` existente; RFC se busca en su forma canónica.

`InvoiceTax { kind, code, base, factor_type, rate, amount }`:

| Campo | Tipo / fuente |
| --- | --- |
| kind | `transferred` o `withheld` |
| code | string, `Impuesto` |
| base | decimal nullable, `Base` |
| factor_type | string nullable, `TipoFactor` |
| rate | decimal nullable, `TasaOCuota` |
| amount | decimal nullable, `Importe` |

`Exento` sin tasa/importe es válido. Retenciones de encabezado pueden no tener
base/factor/tasa. Se extraen los atributos presentes sin inventar ceros. Las listas
fiscales agrupan primero traslados y después retenciones; conservan el orden dentro
de cada grupo, sin sumar ni deduplicar impuestos.

`InvoiceConcept` (orden de array = orden del XML, índice cero para persistir posición):

| Campo | Tipo / atributo |
| --- | --- |
| supplier_code | string nullable / `NoIdentificacion` |
| product_service_code | string / `ClaveProdServ` |
| unit_code | string / `ClaveUnidad` |
| unit | string nullable / `Unidad` |
| description | string / `Descripcion` |
| quantity | decimal positivo / `Cantidad` |
| unit_price | decimal / `ValorUnitario` |
| amount | decimal / `Importe` |
| discount | decimal nullable / `Descuento` |
| taxes | `InvoiceTax[]`, vacío si ausente |

Los códigos no son IDs/SKU del catálogo; no hay vínculos a productos/presentaciones.

`InvoiceDocument`:

| Campo | Tipo / significado |
| --- | --- |
| version, document_type | string, siempre `4.0`, `I` en v1 |
| issuer, receiver | `InvoiceParty` |
| uuid | string canónico del timbre |
| series, folio | string nullable / `Serie`, `Folio` |
| issued_at, issued_date | fecha local lexical y calendario para filtros |
| stamped_at | fecha local lexical nullable / `FechaTimbrado` |
| currency | string de tres letras ASCII mayúsculas / `Moneda`; no catálogo remoto |
| exchange_rate | decimal nullable / `TipoCambio` |
| subtotal, total | decimal / `SubTotal`, `Total` |
| discount | decimal nullable / `Descuento` |
| total_transferred_taxes | decimal nullable / `TotalImpuestosTrasladados` |
| total_withheld_taxes | decimal nullable / `TotalImpuestosRetenidos` |
| taxes | impuestos del encabezado, `InvoiceTax[]` |
| concepts | `InvoiceConcept[]` no vacío |

## Adjuntos, revisión y consulta

`InvoiceAttachmentKind = "xml" | "pdf"`.
`InvoiceAttachmentMetadata { kind, original_name, mime_type, size_bytes, sha256 }`:
nombre base saneado (sin ruta), MIME `application/xml` o `application/pdf`, tamaño
de los bytes originales y SHA-256 hexadecimal **minúsculo** de 64 caracteres.
XML hasta 5 MiB; PDF hasta 15 MiB, con comprobación básica de formato/firma en la
fase nativa. PDF siempre identificado como **adjuntado por el usuario**, sin promesa
de correspondencia fiscal. Nunca se devuelve contenido ni ruta temporal.

- `PurchaseInvoicePreview { review_token, document, xml, pdf }`:
  `review_token` string opaco, `document: InvoiceDocument`, `xml` obligatorio y
  `pdf: InvoiceAttachmentMetadata | null`.
- `PurchaseInvoiceAudit { registered_at, registered_by_user_id, registered_by_name }`:
  auditoría del backend, `registered_at` UTC RFC3339 (`...Z`), usuario autenticado y
  snapshot de nombre. Independiente de las fechas locales del documento.
- `PurchaseInvoiceDetail { id, document, audit, xml, pdf }`: documento completo,
  auditoría y **sólo metadatos** de adjuntos, con la misma nulabilidad de preview.
- `PurchaseInvoiceSummary { id, uuid, issuer, series, folio, issued_at, issued_date,
  currency, total, has_pdf }`: no conceptos, impuestos, metadatos ni BLOB.
- `InvoiceRegistrationOutcome = "created" | "existing"`.
  `InvoiceRegistrationResult { outcome, detail: PurchaseInvoiceDetail }`.
- `InvoiceSupplier { rfc, name }`: selector agrupado por RFC, nombre de la factura
  más reciente por emisión/ID. Buscar sobre **todos** los nombres originales del
  RFC (y RFC), sin modificar snapshots. Resultado sin paginar, orden por clave
  normalizada de nombre ascendente y RFC ascendente para desempatar.

## Operaciones congeladas

Todas requieren sesión local existente. Selección/guardado/apertura se ejecutan en
Rust con capacidades nativas acotadas; no se necesita un plugin JS de filesystem ni
permisos de rutas generales. Dependencias Rust fijadas en Cargo: `roxmltree = 0.21.2`, `tauri-plugin-dialog = 2.7.1` y `tauri-plugin-opener = 2.5.3`; dialog/opener se utilizan del lado nativo, sin permisos generales de filesystem en frontend.

| Función TypeScript → comando | Argumentos externos | Respuesta |
| --- | --- | --- |
| pickInvoiceXml → pick_invoice_xml | ninguno | preview o null al cancelar |
| pickInvoicePreviewPdf → pick_invoice_preview_pdf | `{ reviewToken }` | preview actualizado o null al cancelar |
| discardInvoicePreview → discard_invoice_preview | `{ reviewToken }` | void |
| registerInvoice → register_invoice | `{ reviewToken }` | InvoiceRegistrationResult |
| getPurchaseInvoicesView → get_purchase_invoices_view | `{ filters }` | PurchaseInvoicesView |
| searchInvoiceSuppliers → search_invoice_suppliers | `{ search? }` | InvoiceSupplier[] |
| getPurchaseInvoiceDetail → get_purchase_invoice_detail | `{ invoiceId }` | PurchaseInvoiceDetail |
| attachInvoicePdf → attach_invoice_pdf | `{ invoiceId }` | detail o null al cancelar |
| exportInvoiceAttachment → export_invoice_attachment | `{ invoiceId, kind }` | true si guardado, false si cancelado |
| openInvoicePdf → open_invoice_pdf | `{ invoiceId }` | void |

Cancelación no es error. Cancelar selección PDF conserva el preview previo.
Errores no se convierten en null/false. Abrir PDF usa copia temporal controlada,
visor del sistema, sin DOM PDF; exportar devuelve los bytes originales.

### Listado y filtros

`PurchaseInvoicesViewFilters { supplier_rfc?, number?, date_from?, date_to?, page? }`.
`PurchaseInvoicesView { invoices: PurchaseInvoiceSummary[], total, page, page_size }`.

- AND entre filtros; RFC exacto canónico; número parcial sobre serie, folio,
  serie-folio (separador `-`) o UUID usando clave Unicode de búsqueda, sin distinguir
  mayúsculas. Un solo componente serie/folio sirve también como número visible;
  si no hay ninguno, mostrar UUID.
- Número/search de proveedores: trim, clave `search_key`, máximo 100 caracteres,
  vacío = sin filtro. No interpolar en SQL; escapar comodines LIKE del usuario.
- Fechas inclusivas `YYYY-MM-DD` reales sobre `issued_date`; desde <= hasta.
- Orden fijo `issued_date DESC, issued_at DESC, id DESC` equivale a emisión
  descendente y desempate ID. No sumar monedas distintas.
- Página desde 1, ausencia/null = 1; rechazar 0. Tamaño fijo **25**, sin `page_size`
  de entrada. Total del conjunto filtrado (no de la página). Página fuera del
  conjunto devuelve array vacío y conserva el número solicitado.
- UI reinicia página al cambiar filtros; alternar tabla/tarjetas sólo cambia
  presentación, no consulta, filtros, página u orden. Conservar al volver del detalle.

### Revisión e idempotencia

La selección captura bytes inmutables y metadatos en memoria backend ligados al
token y sesión. No guardar un borrador en DB ni releer una ruta al confirmar.
Reparsear **esos mismos bytes** antes de la transacción de registro; no confiar en
`document` de JS. PDF seleccionado también queda capturado. Tokens nunca aceptan
datos fiscales o rutas arbitrarias como sustituto. El servicio debe acotar memoria
de previews. Su TTL es de 30 minutos; la generación de sesión invalida los tokens al cerrar sesión.

Confirmación atómica de documento/líneas/adjuntos. Índice único UUID:

- UUID nuevo: `created`; UUID y hash XML iguales: `existing`, sin añadir PDF del
  preview silenciosamente. UUID igual con bytes distintos: conflicto, conservar original.
- Conservar token confirmado para reintento idempotente durante su vigencia; si
  expiró/reinició el proceso, volver a seleccionar el mismo XML recupera `existing`.
- Descartar token propio inexistente/ya descartado es idempotente. No permitir
  usar tokens de otra sesión. Fallo de recarga no revierte registro confirmado.
- Adjuntar PDF sólo si falta; mismo hash recupera el detalle, hash distinto no
  reemplaza. Selección y parseo fuera de la transacción, sin afectar caja/inventario.

Persistencia en migración 016: encabezado con UUID único,
hash XML, snapshots y claves de búsqueda; líneas por `(invoice_id, position)`;
adjuntos BLOB únicos `(invoice_id, kind)` con metadatos. El listado no lee BLOB ni
líneas. Detalle lee líneas pero sólo metadatos de adjuntos. Respaldo/restauración
deben conservar bytes y hashes, no referencias a rutas externas.

## Parser y límites de seguridad

`parse_purchase_invoice(&[u8]) -> Result<InvoiceDocument, AppError>` es puro y
acepta UTF-8 (BOM UTF-8 opcional). Namespace CFDI exacto
`http://www.sat.gob.mx/cfd/4`, timbre
`http://www.sat.gob.mx/TimbreFiscalDigital`; prefijos arbitrarios/default válidos.
Campos se resuelven en hijos directos de su padre esperado, no búsquedas globales
que capturen datos de addendas. Emisor/receptor/conceptos/complemento únicos;
impuestos/grupos opcionales pero únicos en cada padre. Exactamente un timbre en
el documento y debe ser hijo directo del complemento del encabezado.
Antes de extraer se rechazan elementos fiscales en namespace incorrecto o fuera
de su padre directo: `Conceptos` sólo contiene `Concepto`, `Impuestos` sólo
grupos `Traslados`/`Retenciones`, y cada grupo sólo sus elementos fiscales CFDI
correspondientes. Addenda y los contenidos de complementos permitidos siguen
siendo opacos; no se exige XSD ni un formato adicional de proveedor.

Límites globales, también sobre contenido desconocido: 5 MiB antes de parsear,
200.000 nodos `roxmltree` (permite 10k conceptos con impuestos), profundidad de
elementos 64 (raíz=1) comprobada iterativamente, 10.000 conceptos, 128 atributos
por elemento, hasta 128 namespaces efectivos por nodo, valores de
atributos/texto/comentario y URI de namespace hasta 16.384 bytes UTF-8,
nombres locales de atributo/elemento y prefijos de namespace hasta 256 bytes. DTD incluso vacío rechazado;
`allow_dtd: false`, `nodes_limit: 200_000`, `entity_resolver: None`.
No red/entidades externas; entidades XML predefinidas y referencias numéricas sí.
Contenido desconocido/addendas se ignora para extracción, nunca para límites.
El parser no devuelve bytes: el backend conserva el buffer original separado.

## Errores estables

Se usa `AppError { category, code, message, field? }` existente; UI decide por code,
no por message. No exponer rutas ni detalles SQL. Los códigos de archivos/DB/IPC
siguientes corresponden al contrato de la entrega.

| Código | Categoría | Condición |
| --- | --- | --- |
| SESSION_REQUIRED | authentication | autenticación local existente requerida |
| INVOICE_FILE_UNREADABLE | validation | no se pudo leer archivo seleccionado |
| INVOICE_FILE_UNSUPPORTED | validation | formato/firma no admitidos, PDF inválido |
| INVOICE_FILE_LIMIT | validation | tamaño de archivo excedido antes de cargarlo |
| INVOICE_INVALID_XML | validation | XML mal formado, entidad desconocida o UTF-8 inválido |
| INVOICE_XML_DTD | validation | declaración DTD prohibida |
| INVOICE_XML_LIMIT | validation | bytes XML, profundidad, nodos, conceptos o longitudes excedidos |
| INVOICE_UNSUPPORTED_VERSION | validation | namespace/raíz/versión no CFDI 4.0 |
| INVOICE_UNSUPPORTED_TYPE | validation | tipo distinto de ingreso I |
| INVOICE_INVALID_UUID | validation | timbre ausente/múltiple/fuera de su padre o UUID inválido |
| INVOICE_REQUIRED_FIELD | validation | campo requerido vacío/ausente o grupo obligatorio ausente |
| INVOICE_AMBIGUOUS_XML | validation | singleton repetido o estructura fiscal/namespace ambiguo |
| INVOICE_INVALID_DECIMAL | validation | sintaxis/precisión/signo/cantidad cero inválidos |
| INVOICE_INVALID_DATE | validation | fecha u hora imposible o formato no admitido |
| INVOICE_INVALID_CURRENCY | validation | moneda fuera de tres letras ASCII mayúsculas |
| INVOICE_INVALID_RFC | validation | RFC fuera del formato acotado |
| INVOICE_INVALID_FILTER | validation | rango de fechas/página/filtro inválido |
| SEARCH_TOO_LONG | validation | búsqueda >100 caracteres, regla compartida existente |
| INVALID_ID | validation | ID fuera de rango, regla compartida existente |
| INVOICE_CONTENT_DIFFERENT | conflict | UUID existente con XML de hash diferente |
| INVOICE_PREVIEW_STALE | conflict | token expirado, no propio o buffer no disponible |
| INVOICE_PDF_EXISTS | conflict | intento de reemplazar PDF por otro hash |
| INVOICE_NOT_FOUND | not_found | factura inexistente |
| INVOICE_ATTACHMENT_NOT_FOUND | not_found | adjunto solicitado ausente |
| INVOICE_EXPORT_FAILED | internal | fallo de escritura al exportar |
| INVOICE_OPEN_FAILED | internal | fallo al preparar/abrir PDF |
| DATABASE_ERROR | database | escritura/consulta fallida, error existente saneado |

Duplicado idéntico **no es un error**: outcome `existing`. Si un documento carece
de `Complemento` se informa `INVOICE_REQUIRED_FIELD`; complemento presente sin
timbre informa `INVOICE_INVALID_UUID`. No prometer verificación fiscal/SAT.

## Verificación pendiente

Fixtures públicos sintéticos: `src-tauri/tests/fixtures/cfdi/{income,minimal}.xml`.
Tests Rust en el parser cubren namespaces, scoped fields/addendas, UUID,
ausencias/Exento, precisión lexical, fecha real, límites, DTD/entidades y BOM/UTF-8.
Tests de modelos cubren serialización y compatibilidad de envelopes; pruebas de
adaptadores propias sin modificar el archivo compartido de pruebas API.

```sh
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --locked --manifest-path src-tauri/Cargo.toml purchase_invoice --lib
pnpm exec vitest run src/api/purchase-invoices.test.ts
pnpm exec tsc --noEmit -p tsconfig.json
pnpm exec tsc --noEmit -p tsconfig.node.json
```

Estos comandos son comprobaciones previstas, no evidencia de ejecución final. Además quedan pendientes QA de migración v16, suite completa y smoke nativo en las plataformas disponibles; consultar el plan y la guía de QA.
