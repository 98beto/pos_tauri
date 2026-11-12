# Plan de ejecución de la siguiente release: v0.3.0

**Estado:** preparación local ejecutada y revisada; integración, candidato y
publicación pendientes de los gates descritos al final de este documento.

**Versión preparada:** `0.3.0`, por las nuevas funciones de caja y facturas.
El tag no existía en GitHub al iniciar esta ejecución; comprobar de nuevo antes
de crearlo, después de aprobar el candidato.

## 1. Punto de partida

- Última release publicada comprobada: **v0.2.1**.
- Rama de desarrollo: `feat/catalog-cash-sessions`; se conserva `main` hasta que
  el usuario pruebe y apruebe integrar los cambios.
- La aplicación declaraba `0.2.1` al iniciar; la preparación la cambia a `0.3.0`,
  sin modificar el esquema objetivo 16.
- Alcance acumulado: icono, baja lógica, lista/tarjetas, apertura/movimientos/cierre
  de caja, historial de ventas en efectivo, facturas documentales, colores de
  movimientos, Manrope/Inter y mejoras de distribución de Inventario/Compras.
- El XML de prueba aportado en `docs` permanece local, fuera de commits y artefactos.
- `Release` acepta dispatch manual sólo desde `main`. No intentar publicar desde
  la rama de desarrollo ni cambiar el workflow para saltar esa condición.

El runbook de referencia es [`../releases.md`](../releases.md). Este documento
concreta su siguiente ejecución; no sustituye sus reglas de firma y recuperación.

## 2. Probar y preparar la integración

1. Revisar los commits y probar en la rama de desarrollo antes de integrarla.
2. Crear un PR a `main` cuando se decida preparar la release. El push de esta
   rama no basta para ejecutar CI: el workflow actual corre en PR y push a `main`.
3. Preparar la versión `0.3.0` dentro del trabajo revisado del PR:
   - `package.json`.
   - `src-tauri/Cargo.toml`.
   - `src-tauri/tauri.conf.json`.
   - Locks regenerados cuando corresponda, incluida la versión raíz de Cargo.
4. Mantener `com.beto98.pos-tauri` y la clave pública de updater existentes.
5. Añadir notas de release basadas en las funciones realmente implementadas.
6. Aprobar CI de Linux y Windows antes de integrar el PR.

No modificar las migraciones 001–016 para preparar la versión. Si la QA revela
un problema de datos, resolverlo con un cambio revisado y repetir las pruebas.

## 3. Gates locales y de CI

Usar las versiones fijadas: Node 24.15.0, pnpm 12.4.1 y Rust 1.98.1. El smoke
local anterior no sustituye comprobar el candidato con esas versiones en CI.

```sh
pnpm install --frozen-lockfile
pnpm validate:version v0.3.0
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --release --manifest-path src-tauri/Cargo.toml
pnpm qa:migration-v16
pnpm qa:migration-v8-v13
git diff --check
```

Después de confirmar el bump y los locks deliberados, las instalaciones y builds
no deben volver a modificarlos. No considerar aprobados como ejecutados los tests
omitidos por falta de Windows o herramientas opcionales en el equipo local.

## 4. Datos y migración

- Verificar bases canónicas de origen 8, 12, 13, 14 y 15 hacia 16, y reapertura de 16.
- Ensayar el candidato sobre una copia cerrada y consistente de los datos reales,
  además de los fixtures sintéticos. Confirmar la versión real del perfil instalado,
  sin inferirla únicamente del número de versión de la aplicación.
- Comprobar stock, códigos, presentaciones, ventas, movimientos y caja retenidos.
- Verificar sesión histórica cerrada para ventas anteriores, sin fondo ni arqueo
  inventados; las instalaciones sin ventas no deben recibir historia ficticia.
- Confirmar hashes/bytes de XML y PDF en una copia SQLite v16 respaldada y reabierta.
- Ensayar restauración durable del backup de migración indicado por el manifiesto,
  usando el ejecutable del candidato y la versión compatible con el origen restaurado.
- El backup anterior a migrar no incluye facturas ni operaciones creadas después:
  guardar una copia consistente v16 antes de ensayar el rollback.

Los marcadores `pos-schema-v13-*` conservan nombres históricos. Usar los campos
de origen/destino del manifiesto, no suponer que esos nombres indican el esquema 13.

## 5. Smoke funcional requerido por plataforma

Realizarlo en perfiles QA aislados, con el candidato que después se publicará:

1. **Productos:** crear/editar, archivar con existencias e historia, buscar códigos,
   restaurar y comprobar las presentaciones que ya estaban inactivas.
2. **Inventario:** Comprar, Contar e historial; acciones a la derecha, listas/tarjetas,
   colores de entradas/salidas y devolución del foco desde el historial.
3. **Caja:** sin apertura no se cobra. Abrir con $500 y vender $150 en efectivo,
   recibidos $200/cambio $50: esperado $650 y una sola entrada de venta por $150.
   Tarjeta/transferencia no incrementan efectivo. Retirar $100: esperado $550;
   cerrar contando $545 y conservar faltante de $5. Reiniciar y comprobar cierre.
4. **Facturas:** seleccionar XML CFDI 4.0 y PDF en el diálogo real; revisar conceptos,
   registrar, filtrar por proveedor/número/fechas, repetir el XML sin duplicados y
   consultar decimales y atributos originales. No cambian existencias ni caja.
5. **Archivos:** cancelar selectores, adjuntar PDF si falta, exportar XML/PDF y
   comparar hashes; abrir el PDF con el visor real del sistema. Éste es un punto
   todavía pendiente de prueba interactiva y debe acreditarse antes de publicar.
6. **Interfaz:** fuentes locales sin Internet ni infracciones CSP, importación azul,
   botones y campos utilizables a 360/1280 px, modales y navegación por teclado.
7. **Persistencia:** reiniciar con caja abierta y datos guardados, comprobar estado
   y recuperación de operaciones ya confirmadas sin duplicarlas.

Registrar plataforma, paquete, SHA y resultado. Las comprobaciones previas en
Chromium con IPC simulado no acreditan los diálogos o el visor nativos ni Windows.
Para estas pruebas se pueden usar fixtures sintéticos; el XML local del usuario
no debe añadirse al repositorio ni publicarse en evidencia compartida.

## 6. Construir el candidato desde main

Después de integrar el PR con versión `0.3.0` y comprobar CI verde:

```sh
git fetch origin
git rev-parse origin/main
gh workflow run release.yml --ref main -f mode=validate
```

Registrar el SHA esperado como `VALIDATE_SHA`. Localizar el run correcto, sin
seleccionar ciegamente el más reciente de otra ejecución:

```sh
gh run list --workflow release.yml --event workflow_dispatch --commit "$VALIDATE_SHA" --json databaseId,headSha,status,conclusion,url
gh run watch "$VALIDATE_RUN_ID" --exit-status
gh run download "$VALIDATE_RUN_ID" --name "release-candidate-$VALIDATE_SHA" --dir "$CANDIDATE_DIR"
```

Definir las variables con el SHA, run ID y directorio QA real antes de ejecutar
los comandos. Comprobar `headSha` y `candidate-provenance.json`; si `main` avanzó
entre la consulta y el dispatch y el run usa otro SHA, seleccionar deliberadamente
el commit/run que se va a probar, sin mezclar evidencias.

Mantener los environments `release` y `release-publication` y las credenciales
ya configuradas según el runbook; no copiar claves a Git ni al plan. `validate`
firma y valida los bundles sin crear una release, aunque requiera sus aprobaciones.

## 7. Validar los mismos bytes que se publicarán

En `CANDIDATE_DIR`, comprobar `SHA256SUMS`, procedencia, versión y firmas siguiendo
el runbook. El candidato debe contener estos diez assets finales:

```text
Linea.POS_0.3.0_amd64.AppImage
Linea.POS_0.3.0_amd64.AppImage.sig
Linea.POS_0.3.0_amd64.deb
Linea.POS_0.3.0_amd64.deb.sig
Linea.POS-0.3.0-1.x86_64.rpm
Linea.POS-0.3.0-1.x86_64.rpm.sig
Linea.POS_0.3.0_x64-setup.exe
Linea.POS_0.3.0_x64-setup.exe.sig
Linea.POS_0.3.0_x64_en-US.msi
latest.json
```

- `SHA256SUMS` y `candidate-provenance.json` son registros internos adicionales,
  no assets públicos de la release.
- Verificar `maintenance-info` y rollback desde AppImage/DEB/RPM/NSIS/MSI reales.
- Probar los canales de actualización AppImage, DEB, RPM y NSIS desde v0.2.1;
  MSI sigue siendo instalación manual y no entra en `latest.json`.
- El artifact dura siete días. Si caduca o cambia el código, generar y aprobar
  un candidato nuevo; no publicar otro build con el run ID de uno anterior.

## 8. Tag y publicación, sólo después de aprobar el candidato

El push del tag **inicia la ruta automática de publicación**, no es un paso neutro.
Realizarlo únicamente tras las pruebas del candidato y decisión de publicar:

```sh
git tag -a v0.3.0 "$VALIDATE_SHA" -m "Linea POS v0.3.0"
git push origin v0.3.0
```

El workflow `Release request` valida el tag y `Release` promueve los bytes ya
aprobados. Comprobar el run y las aprobaciones de publicación. No lanzar otra
publicación manual mientras la automática siga activa.

Si la ruta automática falla por varios candidatos válidos y se decide usar el
run aprobado explícitamente, seguir la recuperación del runbook:

```sh
gh workflow run release.yml --ref main -f mode=publish -f tag=v0.3.0 -f candidate_run_id="$VALIDATE_RUN_ID"
```

`publish` no recompila. No mover/reutilizar el tag ni sustituir assets publicados.

## 9. Comprobaciones posteriores y cierre

- `gh release view v0.3.0` muestra una release publicada, no draft/prerelease.
- `gh release view --json tagName,url` identifica v0.3.0 como la última estable.
- Descargar `latest.json` publicado; verificar versión, URLs, firmas y cuatro canales.
- Comparar los assets públicos con los hashes del candidato aprobado.
- Completar las pruebas de actualización N-1→N del runbook con datos de QA,
  comprobando relanzamiento, fuente local, sesiones e historia documental.
- Registrar tag, SHA, validate run ID, publish run ID, hashes y matriz de pruebas.
- Un defecto posterior se atiende con otra versión; no sobrescribir esta release.

## Checklist de ejecución

- [x] Usuario probó la rama y autorizó commit, push y creación del PR a `main`.
- [ ] Versión 0.3.0 y locks preparados, CI Linux/Windows verde.
- [ ] Migración, reapertura, copia de datos y rollback verificados.
- [ ] Diálogos/archivos/visor nativos y smoke funcional por plataforma completados.
- [ ] Candidato del SHA exacto con firmas y hashes correctos, vigente y aprobado.
- [ ] Tag publicado solamente después de aprobar esos bytes.
- [ ] Publicación y `latest.json` comprobados, actualización y datos de QA intactos.
- [ ] XML del usuario excluido de commits, bundles y artefactos compartidos.

## Registro de ejecución — 12 de noviembre de 2026

### Preparación y revisión local

- Manifiestos sincronizados en `0.3.0`. Cargo regeneró el lock cambiando sólo la
  versión raíz; el lock de pnpm y las dependencias permanecen iguales.
- Notas: [`../release-notes-v0.3.0.md`](../release-notes-v0.3.0.md).
- `maintenance-info` deriva la versión compilada. Los comprobadores Linux,
  NSIS y MSI exigen exactamente la versión del manifiesto del candidato; las
  regresiones rechazan mensajes de versiones antiguas o arbitrarias.
- El runbook exige restaurar con el candidato que migró a 16, y abrir después
  con la versión compatible con el esquema de origen. Su pin de integridad se
  actualizó tras revisar ese cambio; no se relajó el guard.
- Bloques y correcciones pasaron revisiones independientes. No se modificaron
  las migraciones 001–016, la identidad ni las reglas de firma/publicación.

### Gates locales acreditados

Herramientas: Node `24.15.0`, pnpm `12.4.1`, Rust `1.98.1`.

| Comprobación | Resultado |
| --- | --- |
| `pnpm install --frozen-lockfile` | Aprobado |
| `pnpm validate:version v0.3.0` | Aprobado |
| `pnpm test` | 384 frontend, 99 guard, 12 versión y 133 release aprobadas |
| `pnpm build` | Aprobado; frontend en `dist/` |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | Aprobado |
| `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings` | Aprobado |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml` | 216 biblioteca + 5 CLI aprobadas; 2 harnesses ignorados, ejecutados aparte |
| `cargo test --locked --release --manifest-path src-tauri/Cargo.toml` | 217 biblioteca + 5 CLI aprobadas; 2 harnesses ignorados, ejecutados aparte |
| `pnpm qa:migration-v16` | Aprobado, fuentes 8/12/13/14/15 a 16 |
| `pnpm qa:migration-v8-v13` | Aprobado, alias con las mismas fuentes y destino 16 |
| `git diff --check` | Aprobado |

Los hashes de ambos locks no cambiaron durante las verificaciones. Las dos
omisiones de release son el fixture GUI Windows/PowerShell y la integración
Tauri/minisign opcional; no se acreditan como aprobadas. Cargo mostró un aviso
de colisión de nombres de artefactos de biblioteca en release, sin fallar el build.

Ambas QA verificaron backup, reapertura y restauración sintéticos, además de bytes
y hashes de adjuntos en backup v16. Los informes temporales quedaron en
`/tmp/pos-tauri-v8-v13-qa-lZJ2Hq` y `/tmp/pos-tauri-v8-v13-qa-gCgAPu`.

El binario release arrancó durante 25 segundos con datos/configuración aislados en
`/tmp/opencode/pos-tauri-smoke.czfkEN`; creó una base con `user_version=16`,
`quick_check=ok` y sin violaciones FK. La terminación por timeout fue controlada.
Esto acredita arranque local, no recorridos interactivos ni instaladores firmados.

### Gates operativos pendientes

La consulta de GitHub confirmó `v0.2.1` como última release estable y disponibilidad
inicial del tag `v0.3.0`. También detectó requisitos del runbook sin configurar:
`main` no tiene protección ni reglas aplicables, y `release-publication` no tiene
reviewers ni restricción de rama. La inmutabilidad de releases está habilitada;
existe una ruleset activa de tags y el environment `release` tiene reviewer y
restricción a `main`. Corregir los requisitos pendientes antes de la publicación.

Faltan CI Linux/Windows, revisión e integración del PR, candidato firmado desde
`main`, copia cerrada de datos reales, QA nativa por plataforma, rollback del
candidato y aprobación de esos bytes. Los XML locales no forman parte de los
cambios preparados. No se ha creado un tag ni publicado una release.
