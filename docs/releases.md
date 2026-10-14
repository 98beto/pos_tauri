# Operacion de releases

Esta guia corresponde al repositorio publico `98beto/pos_tauri`, la rama `main` y los workflows `.github/workflows/release-request.yml` y `.github/workflows/release.yml`. El canal estable del updater es:

```text
https://github.com/98beto/pos_tauri/releases/latest/download/latest.json
```

El workflow admite `validate` y `publish`. `validate` compila el `HEAD` de `main`, firma y verifica todos los bundles y conserva un artifact candidato durante siete dias sin crear ni modificar releases. `publish` compila el commit inmutable de un tag `vX.Y.Z`, crea o reutiliza una release en borrador y la publica automaticamente solo despues de superar todos los gates.

## Prerrequisitos locales

- Acceso de escritura al repositorio y permiso para crear tags.
- Rama local `main` actualizada, limpia y basada en `origin/main`.
- Node.js 24.15.0, pnpm 12.4.1 y Rust 1.98.1 con `rustfmt` y `clippy`.
- Dependencias de sistema de Tauri 2 si se ejecutan builds nativos locales.
- Cambios de la release ya integrados en `main` y CI verde.
- Variable, environment protegido y secrets de firma configurados en GitHub antes de crear el tag.
- Ruleset inmutable para tags `v*` y GitHub immutable releases habilitados. Una release no debe comenzar sin ambas protecciones.

Las versiones se fijan en `.node-version`, `package.json`, `rust-toolchain.toml` y los workflows. Una actualizacion de toolchain es un cambio independiente y debe pasar CI antes de preparar una release.

El identificador Tauri `com.beto98.pos-tauri` es un invariante de release. No cambiarlo: determina la identidad instalada y la ubicacion de los datos; modificarlo puede hacer que SQLite parezca perdido o crear una segunda instalacion.

## Clave del updater

### Generacion y respaldo inicial

Generar la pareja una sola vez, fuera del repositorio y desde una terminal privada:

```bash
pnpm tauri signer generate -w ~/.tauri/linea-pos-updater.key
```

Usar el prompt interactivo para una contrasena robusta. No pasar la contrasena como argumento, no usar `--ci` para produccion y no pegar claves en comandos, historial, tickets o logs. El comando crea la clave privada en la ruta indicada y una clave publica asociada; ninguno de esos archivos debe copiarse dentro del repositorio.

Conservar al menos dos respaldos cifrados e independientes de la clave privada y su contrasena, con acceso restringido y una restauracion probada. Perder la clave impide actualizar los clientes que confian en ella. Nunca almacenar la clave privada ni la contrasena en Git, `.env`, artefactos de Actions o notas de release.

### Configuracion en GitHub

En `Settings > Secrets and variables > Actions`:

- Crear la variable de repositorio `TAURI_UPDATER_PUBLIC_KEY` con el contenido completo de la clave publica.

En `Settings > Environments`:

- Crear el environment `release`, exigir required reviewers y limitar deployments a `main` (la rama default).
- Crear preferiblemente como environment secrets de `release` el secret `TAURI_SIGNING_PRIVATE_KEY` con el contenido completo de la clave privada y `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` con su contrasena. Si temporalmente son repository secrets, el environment protegido sigue siendo obligatorio.

Introducir los valores solamente en los campos protegidos de la interfaz. Evitar comandos CLI que incluyan el valor en argumentos, porque pueden quedar en el historial o la lista de procesos. La variable publica se inyecta en un overlay efimero durante la release; `src-tauri/tauri.conf.json` no contiene una clave de produccion ni un valor sustituible que pueda publicarse por accidente.

Para probar bundles antes del tag, ejecutar manualmente `Release` desde `main` con `mode=validate`. El modo `publish` requiere un tag existente y puede publicar la release; comprobar siempre el modo seleccionado.

## Preparar una version

1. Elegir una version SemVer sin prefijo, por ejemplo `1.2.3`.
2. Cambiarla de forma identica en `package.json`, `src-tauri/Cargo.toml` y `src-tauri/tauri.conf.json`.
3. Regenerar los locks con las herramientas fijadas cuando sea necesario; no editar dependencias bloqueadas a mano.
4. Revisar y confirmar cualquier cambio esperado en `pnpm-lock.yaml` y `src-tauri/Cargo.lock`. El paquete raiz de Cargo debe reflejar la nueva version.
5. Ejecutar las validaciones usando el tag que se creara.

Comandos para el ejemplo:

```bash
pnpm install
cargo check --manifest-path src-tauri/Cargo.toml
pnpm install --frozen-lockfile
pnpm validate:version v1.2.3
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml --release
git diff --exit-code -- pnpm-lock.yaml src-tauri/Cargo.lock
```

El ultimo comando se ejecuta despues de confirmar los cambios de version y locks; comprueba que instalaciones y pruebas no los modificaron de nuevo. `pnpm validate:version` rechaza formatos distintos de `vX.Y.Z` y discrepancias entre los tres archivos.

## Runbook v0.2.0: copia cerrada de esquema 8

Este procedimiento prepara los bloques 13 y 14. No se ejecuta sobre la base activa y no autoriza una publicacion. La aplicacion usa `pos.db` dentro del directorio de datos de `com.beto98.pos-tauri`; el backup y los marcadores quedan en ese mismo directorio.

### Evidencia y candidato exacto

Crear fuera del repositorio un directorio de trabajo restringido, separar los datos QA de la evidencia compartible y registrar el commit candidato antes de iniciar. No guardar datos de negocio, tokens ni secretos en la evidencia:

```bash
export RELEASE_TAG=v0.2.0
export CANDIDATE_SHA="$(git rev-parse HEAD^{commit})"
export RELEASE_WORK_ROOT=/ruta/restringida/linea-pos-v0.2.0
export EVIDENCE_DIR="$RELEASE_WORK_ROOT/evidence"
export QA_SECURE_DIR="$RELEASE_WORK_ROOT/private-data"
install -d -m 700 "$RELEASE_WORK_ROOT" "$EVIDENCE_DIR" "$QA_SECURE_DIR"
printf '%s\n' "$CANDIDATE_SHA" > "$EVIDENCE_DIR/candidate-sha.txt"
pnpm validate:version "$RELEASE_TAG" | tee "$EVIDENCE_DIR/version.txt"
```

El commit debe estar integrado en `main` antes de ejecutar GitHub Actions. Lanzar `Release` desde `main` con `mode=validate`, nunca con `publish`, y dejar constancia del run ID, su `headSha`, conclusion y SHA-256 del artifact `release-candidate`. `validate` no crea ni modifica releases. Descargar el artifact candidato generado por ese run, no compilar otro commit para la prueba:

```bash
gh workflow run release.yml --ref main -f mode=validate
gh run list --workflow release.yml --event workflow_dispatch --limit 5
gh run watch VALIDATE_RUN_ID --exit-status
gh run view VALIDATE_RUN_ID --json databaseId,headSha,conclusion,url \
  > "$EVIDENCE_DIR/validate-run.json"
test "$(jq -r .headSha "$EVIDENCE_DIR/validate-run.json")" = "$CANDIDATE_SHA"
gh run download VALIDATE_RUN_ID -n release-candidate -D "$EVIDENCE_DIR/release-candidate"
(cd "$EVIDENCE_DIR/release-candidate" && sha256sum -- * | sort) \
  > "$EVIDENCE_DIR/release-candidate.sha256"
```

Si `headSha` no coincide, si cambia el commit o si cualquier comprobacion falla, invalidar el candidato y repetir `validate` y toda la prueba. La copia QA se migra con el candidato descargado. Produccion se migra unicamente con el artefacto final publicado, compilado desde ese mismo commit etiquetado; `publish` vuelve a compilar y no se presupone que sus hashes sean reproducibles respecto a `validate`.

### Obtener una copia v8 cerrada

1. Programar una ventana sin operaciones y cerrar Linea POS en todos los procesos del equipo origen.
2. Confirmar que no queda ningun proceso de Linea POS. No copiar una base mientras la aplicacion siga abierta.
3. Localizar `pos.db` en el directorio de datos de `com.beto98.pos-tauri` y definir `SOURCE_DB` sin mover ni modificar el original.
4. Comprobar que es esquema 8, integro y sin claves foraneas invalidas.
5. Crear una instantanea consistente con la CLI de SQLite. Aunque la aplicacion este cerrada, no usar `cp` sobre un archivo que pueda tener WAL pendiente.

```bash
export SOURCE_DB=/ruta/absoluta/al/directorio-de-datos/pos.db
export CLOSED_V8_COPY="$QA_SECURE_DIR/pos-v8-closed-copy.db"
test "$(sqlite3 "$SOURCE_DB" 'PRAGMA user_version;')" = 8
test "$(sqlite3 "$SOURCE_DB" 'PRAGMA quick_check;')" = ok
test -z "$(sqlite3 "$SOURCE_DB" 'PRAGMA foreign_key_check;')"
sqlite3 "$SOURCE_DB" ".timeout 5000" ".backup '$CLOSED_V8_COPY'"
chmod 600 "$CLOSED_V8_COPY"
test "$(sqlite3 "$CLOSED_V8_COPY" 'PRAGMA user_version;')" = 8
test "$(sqlite3 "$CLOSED_V8_COPY" 'PRAGMA quick_check;')" = ok
test -z "$(sqlite3 "$CLOSED_V8_COPY" 'PRAGMA foreign_key_check;')"
```

Si la copia sale del equipo autorizado, anonimizarla mediante un procedimiento aprobado o no transferirla. No adjuntar la base, el backup ni el manifiesto completo a Actions, issues o evidencia compartida.

Guardar comparables previos sin valores personales. Como minimo, registrar conteos y stock por ID; conservar por separado una lista autorizada de tickets historicos a revisar:

```bash
sqlite3 -readonly -header -csv "$CLOSED_V8_COPY" \
  'SELECT "products" AS table_name, COUNT(*) AS rows FROM products UNION ALL SELECT "sales", COUNT(*) FROM sales UNION ALL SELECT "sale_details", COUNT(*) FROM sale_details UNION ALL SELECT "inventory_movements", COUNT(*) FROM inventory_movements;' \
  > "$EVIDENCE_DIR/v8-counts.csv"
sqlite3 -readonly -header -csv "$CLOSED_V8_COPY" \
  'SELECT id, stock, inventory_cost_cents FROM products ORDER BY id;' \
  > "$EVIDENCE_DIR/v8-product-state.csv"
```

### Migrar solamente la copia

Usar una maquina QA sin acceso al directorio de datos real. En Linux, el AppImage puede aislarse fijando `XDG_DATA_HOME`; confirmar la ruta creada antes de abrir cualquier otro binario:

```bash
export QA_ROOT="$QA_SECURE_DIR/runtime"
install -d -m 700 "$QA_ROOT"
export XDG_DATA_HOME="$QA_ROOT/data"
export QA_APP_DATA="$XDG_DATA_HOME/com.beto98.pos-tauri"
install -d -m 700 "$QA_APP_DATA"
cp "$CLOSED_V8_COPY" "$QA_APP_DATA/pos.db"
chmod 600 "$QA_APP_DATA/pos.db"
XDG_DATA_HOME="$XDG_DATA_HOME" "$EVIDENCE_DIR/release-candidate/Linea.POS_0.2.0_amd64.AppImage"
```

En Windows usar una VM o usuario QA desechable y confirmar mediante la ruta mostrada por el sistema que `pos.db` pertenece a ese perfil aislado. No cambiar el identificador Tauri para la prueba y no apuntar el candidato al perfil de produccion.

Tras el primer inicio, cerrar completamente la aplicacion y validar:

```bash
export MIGRATED_DB="$QA_APP_DATA/pos.db"
test "$(sqlite3 "$MIGRATED_DB" 'PRAGMA user_version;')" = 12
test "$(sqlite3 "$MIGRATED_DB" 'PRAGMA quick_check;')" = ok
test -z "$(sqlite3 "$MIGRATED_DB" 'PRAGMA foreign_key_check;')"
mapfile -t V8_BACKUPS < <(find "$QA_APP_DATA" -maxdepth 1 -name 'pos-v8-before-v0.2.0-*.db' -print)
mapfile -t COMPLETED_MANIFESTS < <(find "$QA_APP_DATA" -maxdepth 1 -name 'pos-v0.2.0-migration-completed-*.json' -print)
test "${#V8_BACKUPS[@]}" -eq 1
test "${#COMPLETED_MANIFESTS[@]}" -eq 1
test ! -e "$QA_APP_DATA/pos-v0.2.0-migration-pending.json"
```

Debe existir exactamente un backup y un manifiesto completado para este intento, y no debe quedar `pos-v0.2.0-migration-pending.json`. Leer la ruta desde el manifiesto, no elegir el backup por fecha o nombre:

```bash
export MANIFEST="${COMPLETED_MANIFESTS[0]}"
export V8_BACKUP="$(jq -er 'select(.source_version == 8 and .target_version == 12) | .backup_path' "$MANIFEST")"
test -f "$V8_BACKUP"
test "$(sqlite3 "$V8_BACKUP" 'PRAGMA user_version;')" = 8
test "$(sqlite3 "$V8_BACKUP" 'PRAGMA quick_check;')" = ok
test -z "$(sqlite3 "$V8_BACKUP" 'PRAGMA foreign_key_check;')"
```

Repetir los CSV de conteos y stock sobre `MIGRATED_DB` y compararlos con los de v8. Verificar manualmente una muestra de tickets historicos, incluidos efectivo y cambio. Abrir y cerrar otra vez `v0.2.0`; confirmar que permanece en esquema 12 y que no aparece un segundo backup. En la copia migrada probar compra, venta, ajuste y valoracion, y registrar resultado, SO, arquitectura, formato del instalador y hashes. Las operaciones funcionales de prueba ocurren despues de las comparaciones historicas y nunca se esperan en el backup v8.

### Ensayo de rollback durable a v0.1.0

Ejecutar este ensayo sobre el directorio QA anterior. No intentar abrir esquema 12 con `v0.1.0`.

1. Cerrar completamente `v0.2.0` y confirmar que no queda ningun proceso.
2. Crear y validar una copia de diagnostico del esquema 12.
3. Volver a leer `backup_path` del manifiesto completado o, tras un fallo, de `pos-v0.2.0-migration-pending.json`.
4. Validar nuevamente el backup v8 antes de restaurar.
5. Restaurar primero a un archivo temporal en el mismo directorio y persistirlo. Antes de promoverlo, mover `pos.db`, `pos.db-wal` y `pos.db-shm` existentes a nombres de cuarentena en ese mismo directorio y sincronizar el directorio. Nunca puede coexistir el v8 promovido con sidecars del v12.
6. Si habia marcador pendiente, archivarlo como evidencia fuera del nombre activo solo despues de validar la restauracion; de lo contrario bloquearia correctamente el siguiente inicio.
7. Conservar la base v12 y sus sidecars en cuarentena, junto con la copia diagnostica y los marcadores archivados, hasta cerrar la investigacion. No borrarlos como parte del rollback.

La finalizacion de migracion y restauracion publica primero un manifiesto completado independiente: escribe y sincroniza un temporal exclusivo y lo publica durablemente con un nombre final. Solo entonces retira el marcador activo. Si un corte deja ambos archivos, el siguiente inicio solo retira el activo despues de comprobar que los manifiestos coinciden y volver a sincronizar el completado; si difieren o cualquier sincronizacion falla, bloquea la apertura.

```bash
export MIGRATION_STATE="$MANIFEST"
if test -f "$QA_APP_DATA/pos-v0.2.0-migration-pending.json"; then
  export MIGRATION_STATE="$QA_APP_DATA/pos-v0.2.0-migration-pending.json"
fi
export V8_BACKUP="$(jq -er 'select(.source_version == 8 and .target_version == 12) | .backup_path' "$MIGRATION_STATE")"
test -f "$V8_BACKUP"
test "$(sqlite3 "$V8_BACKUP" 'PRAGMA user_version;')" = 8
test "$(sqlite3 "$V8_BACKUP" 'PRAGMA quick_check;')" = ok
test -z "$(sqlite3 "$V8_BACKUP" 'PRAGMA foreign_key_check;')"
sqlite3 "$MIGRATED_DB" ".backup '$QA_APP_DATA/pos-v12-diagnostic.db'"
test "$(sqlite3 "$QA_APP_DATA/pos-v12-diagnostic.db" 'PRAGMA user_version;')" = 12
export RESTORE_TMP="$QA_APP_DATA/.pos-v8-restore.tmp"
export RESTORE_ID="$(date +%s)-$$"
export QUARANTINE_DB="$QA_APP_DATA/.pos-quarantine-$RESTORE_ID.db"
export QUARANTINE_WAL="$QA_APP_DATA/.pos-quarantine-$RESTORE_ID-sidecar-wal"
export QUARANTINE_SHM="$QA_APP_DATA/.pos-quarantine-$RESTORE_ID-sidecar-shm"
rm -f -- "$RESTORE_TMP"
sqlite3 "$V8_BACKUP" ".backup '$RESTORE_TMP'"
test "$(sqlite3 "$RESTORE_TMP" 'PRAGMA user_version;')" = 8
test "$(sqlite3 "$RESTORE_TMP" 'PRAGMA quick_check;')" = ok
test -z "$(sqlite3 "$RESTORE_TMP" 'PRAGMA foreign_key_check;')"
chmod 600 "$RESTORE_TMP"
sync -f "$RESTORE_TMP"
test -f "$MIGRATED_DB" && mv -- "$MIGRATED_DB" "$QUARANTINE_DB"
test ! -f "$MIGRATED_DB-wal" || mv -- "$MIGRATED_DB-wal" "$QUARANTINE_WAL"
test ! -f "$MIGRATED_DB-shm" || mv -- "$MIGRATED_DB-shm" "$QUARANTINE_SHM"
sync -f "$QA_APP_DATA"
mv -- "$RESTORE_TMP" "$MIGRATED_DB"
sync -f "$QA_APP_DATA"
if test -f "$QA_APP_DATA/pos-v0.2.0-migration-pending.json"; then
  mv -- "$QA_APP_DATA/pos-v0.2.0-migration-pending.json" \
    "$EVIDENCE_DIR/pending-marker-from-rollback-test.json"
fi
test "$(sqlite3 "$MIGRATED_DB" 'PRAGMA user_version;')" = 8
test "$(sqlite3 "$MIGRATED_DB" 'PRAGMA quick_check;')" = ok
test -z "$(sqlite3 "$MIGRATED_DB" 'PRAGMA foreign_key_check;')"
```

Si falla cualquier movimiento de cuarentena, no promover `RESTORE_TMP`: conservar el namespace parcial y restaurar primero todos los nombres originales. Si falla la promocion, su sincronizacion, la validacion o el archivado del marcador, aislar el v8 promovido como `.pos-restore-failed-*.db`, devolver sidecars y base v12 desde cuarentena (sidecars primero, base al final), sincronizar el directorio y conservar el archivo fallido. Si cualquiera de esos pasos de rollback falla, la recuperacion esta incompleta: no iniciar ninguna version y escalar usando backup, marcador y cuarentenas.

En Windows no se acepta un procedimiento equivalente basado solo en `Move-Item`. El backend sincroniza el contenido de los temporales nuevos y publica cada movimiento critico del namespace con `MoveFileExW(..., MOVEFILE_WRITE_THROUGH)` en el mismo volumen, sin permitir copia ni reemplazo para los manifiestos nuevos. Windows no admite `FlushFileBuffers` sobre handles de directorio; por eso el manifiesto completado durable es la autoridad. Si un corte deja tambien el marcador activo, el siguiente inicio exige que ambos coincidan antes de retirarlo; si difieren o falla una publicacion write-through, la apertura queda bloqueada. Ejecutar el ensayo Windows solo en un volumen local NTFS con la aplicacion cerrada y mediante esta primitiva probada, no mediante una sustitucion manual de archivos.

Instalar o ejecutar el artefacto publicado e inmutable de `v0.1.0` con ese mismo perfil QA. Verificar productos, stock, ventas y tickets previos, cerrar y reabrir. Comparar otra vez con `v8-counts.csv` y `v8-product-state.csv`. Registrar que toda operacion creada despues de migrar a `v0.2.0` se pierde al restaurar el backup; el rollback de produccion debe decidirse antes de reanudar operacion prolongada.

### Despliegue controlado y publicacion

No continuar hasta que `validate`, migracion, reapertura y rollback hayan pasado sobre el mismo candidato. Si cambia un byte o commit, repetir todo.

1. Congelar UI, contratos, migraciones `009`-`012` y commit candidato.
2. Confirmar CI verde, `pnpm validate:version v0.2.0` y que el SHA validado pertenece a `main`.
3. Crear el tag anotado inmutable `v0.2.0` exactamente sobre `CANDIDATE_SHA` y verificarlo antes de enviarlo.
4. Autorizar `mode=publish` solamente despues de la aprobacion operacional. Este modo puede publicar y no forma parte del ensayo local.
5. Confirmar que el run de publicacion usa el SHA etiquetado; comparar los SHA-256 locales y remotos de ese mismo run y validar firmas y `latest.json`.
6. Instalar primero el artefacto final en un equipo controlado, confirmar backup, manifiesto, esquema 12, funciones principales y reapertura; despues liberar los demas equipos.

```bash
test "$(git rev-parse HEAD^{commit})" = "$CANDIDATE_SHA"
git tag -a v0.2.0 "$CANDIDATE_SHA" -m "Linea POS v0.2.0"
git rev-parse 'v0.2.0^{commit}'
git push origin v0.2.0
# Solo con autorizacion explicita; publica una release:
gh workflow run release.yml --ref main -f mode=publish -f tag=v0.2.0
```

### Checklist y evidencia minima

- [ ] Aplicacion de origen cerrada y copia consistente con `user_version=8`, `quick_check=ok` y cero filas de `foreign_key_check`.
- [ ] Commit SHA, validate run URL/ID, plataforma, instalador y SHA-256 registrados.
- [ ] `headSha` de `mode=validate` igual al commit candidato.
- [ ] Conteos, stock, costos, tickets, efectivo, cambio, IDs, fechas y tokens historicos conservados.
- [ ] Esquema final 12 integro y reapertura sin otra migracion ni otro backup.
- [ ] Un unico manifiesto completado apunta al backup exacto; backup v8 valido y protegido.
- [ ] Compra, venta, ajuste y valoracion pasan solamente sobre la copia QA.
- [ ] Rollback restaura durablemente esquema 8 y `v0.1.0` abre y reabre los datos esperados.
- [ ] Perdida esperada de operaciones posteriores a la migracion entendida y ventana de decision acordada.
- [ ] Sin bases, backups, manifiestos con rutas sensibles, datos personales, tokens ni secretos en evidencia compartida.
- [ ] Cualquier cambio posterior al ensayo invalida el candidato y repite `validate`, migracion y rollback.
- [ ] Tag y `publish` aprobados explicitamente y ejecutados solo despues de completar los puntos anteriores.

Crear un commit normal mediante pull request. Cuando este en `main`, crear el tag anotado sobre el commit exacto y publicarlo:

```bash
git switch main
git pull --ff-only origin main
git tag -a v1.2.3 -m "Linea POS v1.2.3"
git push origin v1.2.3
```

No mover ni reutilizar un tag enviado. El workflow exige que su commit pertenezca a la historia de `origin/main` y que no exista una release ya publicada para ese tag.

## Ejecucion del workflow

Un push de `v*` ejecuta primero `Release request`, que no tiene secrets ni permisos de escritura. Al terminar correctamente, `Release` se carga desde la rama default mediante `workflow_run`, obtiene exactamente un tag SemVer que apunte a `workflow_run.head_sha`, exige que el SHA pertenezca a `origin/main` y compila ese SHA.

`workflow_dispatch` solo puede lanzarse seleccionando `main`:

- `mode=validate` usa el `HEAD` de `origin/main`, deriva el tag desde `package.json`, construye el candidato completo y deja omitido el job con `contents: write`.
- `mode=publish` requiere un tag `vX.Y.Z` existente y sirve para recuperar o reintentar la publicacion normal.

Cada job genera `dist` con `pnpm build` antes de cualquier acceso a secrets. Linux compila sin la clave privada, sanea el AppImage y expone la clave solamente al paso que firma los tres instaladores finales. NSIS tambien compila sin la clave y la expone solo al firmador posterior. MSI no recibe secretos ni configuracion del updater. El overlay efimero anula `build.beforeBuildCommand`, por lo que Vite o sus plugins no heredan credenciales de firma.

Windows MSI, que no recibe secretos, puede compilar mientras espera la aprobacion. Tras la aprobacion de los required reviewers, se ejecutan Linux y Windows NSIS. El job `assemble`, con `contents: read`, se ejecuta en ambos modos y:

1. Canonicaliza los nombres de assets.
2. Verifica criptograficamente las cuatro firmas locales.
3. Genera `latest.json` y valida version, nombres, URLs, firmas y conjunto exacto.
4. Sube un artifact `release-candidate` con nueve assets nativos y `latest.json`.

Solo en `publish`, un job separado con `contents: write` confirma de nuevo tag y commit, crea o reutiliza el borrador, sube el candidato, vuelve a descargarlo, compara los SHA-256 de los diez archivos y publica la release como `latest`.

## Artefactos esperados

La config base mantiene `createUpdaterArtifacts=false`, por lo que builds normales no requieren firma. Para `1.2.3`, el workflow firma los cuatro canales updater y exige estos nueve assets nativos antes de agregar `latest.json`:

```text
Linea.POS_1.2.3_amd64.AppImage
Linea.POS_1.2.3_amd64.AppImage.sig
Linea.POS_1.2.3_amd64.deb
Linea.POS_1.2.3_amd64.deb.sig
Linea.POS-1.2.3-1.x86_64.rpm
Linea.POS-1.2.3-1.x86_64.rpm.sig
Linea.POS_1.2.3_x64-setup.exe
Linea.POS_1.2.3_x64-setup.exe.sig
Linea.POS_1.2.3_x64_en-US.msi
latest.json
```

Una version futura cambia solo el segmento de version. Los basenames, producto, arquitectura y locale deben coincidir exactamente. No son validos los formatos heredados `*.AppImage.tar.gz`, `*.AppImage.tar.gz.sig`, `*.nsis.zip` o `*.nsis.zip.sig`.

| Plataforma | Instalacion manual | Updater |
| --- | --- | --- |
| Linux | AppImage, `.deb`, `.rpm` | AppImage, DEB y RPM, cada uno con su `.sig` |
| Windows | NSIS `setup.exe`, MSI | NSIS `setup.exe` y su `.sig` |

`latest.json` contiene solamente `linux-x86_64-appimage`, `linux-x86_64-deb`, `linux-x86_64-rpm` y `windows-x86_64-nsis`. Cada entrada apunta a su formato nativo y usa la firma del mismo archivo. MSI permanece manual y nunca aparece como URL del updater.

AppImage se actualiza reemplazando el ejecutable. DEB usa `dpkg -i` y RPM usa `rpm -U`; ambos pueden requerir elevacion administrativa. El usuario confirma antes de descargar e instalar. Actualizar o desinstalar el paquete no elimina `pos.db`, almacenada en el directorio de datos de `com.beto98.pos-tauri`.

El frontend del updater se compila con `VITE_UPDATER_ENABLED=true` solamente para AppImage, DEB, RPM y NSIS. MSI y builds de produccion sin overlay no registran ni ofrecen el updater.

## Revision, publicacion y cancelacion

El borrador solo existe durante la fase final y puede durar pocos segundos. Si se necesita observarlo, abrir `Releases` mientras corre `Atomically publish verified candidate` y comprobar:

- Tag, titulo y notas correctos.
- Los nueve assets nativos y `latest.json`, sin residuos de otro intento.
- Version igual al tag, fecha valida, notas no vacias, cuatro plataformas y URLs de esa release.
- Firmas no vacias y `.sig` emparejados exactamente con AppImage, DEB, RPM y NSIS.

La inspeccion visual no reemplaza los gates: el workflow verifica firmas con `minisign`, compara el conjunto remoto con el local y valida el manifiesto descargado. No pulsar `Publish release` manualmente; puede saltarse comprobaciones pendientes. El workflow publica por si mismo.

Para cancelar antes de publicar:

1. Cancelar la ejecucion desde `Actions > Release` cuanto antes.
2. Comprobar en `Releases` si el job final creo un borrador.
3. Eliminar el borrador desde la interfaz si se abandona, o conservarlo para que un rerun lo reconcilie.
4. Mantener el tag si solo se corregira una causa externa. Si el codigo era incorrecto, preparar otra version; no mover el tag.

Cancelar no revierte una release que ya alcanzo el paso final. Si fue publicada, no reemplazar sus assets: tratarla como inmutable y publicar una version correctiva.

## Recuperacion de fallos

- Antes del borrador: corregir variables, secrets, runner o incidencia transitoria y usar `Re-run failed jobs`, o `workflow_dispatch` con el mismo tag.
- Despues del borrador: conservarlo y reintentar. El workflow elimina assets remotos residuales, sube el conjunto local exacto y revalida.
- Por version, tests, lockfiles o compilacion: corregir en `main`, incrementar otra vez la version y crear un tag nuevo. No retaggear.
- Por intervencion manual que publico algo parcial: no confiar en ese canal. Retirarlo de `latest` si es imprescindible, investigar y publicar una version correctiva; clientes pueden haber descargado ya el manifiesto.
- Por secret o variable ausente: configurarlo en GitHub y reintentar; nunca agregarlo al repositorio.

Una release incompleta que sigue en draft no es visible mediante `/releases/latest` y no sustituye el `latest.json` estable anterior.

## Primera release con updater

Una aplicacion anterior al plugin updater no puede descubrir la primera release habilitada. Esa version se instala manualmente y sirve como N-1. La prueba real requiere publicar despues una segunda version habilitada N:

1. Publicar la primera release habilitada y comprobar que `latest.json` es publico.
2. Instalar AppImage, DEB, RPM y NSIS en sus sistemas de prueba.
3. Crear datos representativos y cerrar/reabrir la aplicacion.
4. Preparar y publicar la siguiente version con el workflow.
5. Ejecutar la lista N-1 a N en los cuatro canales.

## Prueba real N-1 a N

1. Instalar N-1 desde sus assets publicados en cuatro entornos: AppImage, DEB, RPM y NSIS.
2. Probar rutas con espacios para AppImage o descarga cuando la plataforma lo permita.
3. Crear propietario, productos, inventario y una venta; anotar totales y conteos.
4. Dejar un carrito no cobrado y comprobar su advertencia.
5. Confirmar que N-1 detecta N al iniciar, muestra versiones/notas y no descarga antes de `Actualizar ahora`.
6. Elegir `Despues`, seguir usando el POS y recuperar el flujo mediante la consulta manual.
7. Reiniciar N-1 y confirmar que vuelve a ofrecer N.
8. Actualizar sin checkout ni mutacion del carrito en curso.
9. En Windows aceptar que NSIS puede cerrar la aplicacion; `downloadAndInstall` no necesariamente regresa a la UI.
10. En Linux confirmar reemplazo de AppImage o elevacion para `dpkg -i`/`rpm -U`, y comprobar el relanzamiento cuando la llamada regrese.
11. Abrir N, verificar su version y confirmar que SQLite, propietario, productos, inventario y ventas sobrevivieron.
12. Repetir con usuario estandar, confirmar la elevacion esperada para DEB/RPM y registrar cancelaciones o fallos.

Probar por separado en un entorno controlado: `latest.json` malformado, firma alterada o de otro archivo, interrupcion de red y cierre durante la descarga. La instalacion vigente debe permanecer utilizable y un reintento posterior debe ser posible.

No alterar la release estable para estas pruebas. Un draft no es accesible anonimamente por `/releases/latest/download/latest.json` y produccion tiene el endpoint estable embebido. Probar un draft requiere una build QA separada con otro endpoint publico/controlado y artefactos firmados, o infraestructura dinamica de staging. Con el repositorio actual, los gates validan el borrador y la prueba N-1 a N estable se completa inmediatamente despues de publicar N. Los casos destructivos solo se ejecutan en QA.

Registrar SO, arquitectura, versiones, instalador, ruta, privilegios, resultado y logs sanitizados; nunca claves ni tokens.

## Rotacion de claves

La clave publica queda embebida en cada aplicacion. Un cliente que solo confia en la clave anterior rechazara una release firmada solo con la nueva. `latest.json` tiene una firma por plataforma y el endpoint estatico de GitHub no sirve firmas diferentes por cliente.

- No rotar periodicamente una clave protegida; respaldarla y limitar acceso favorece la continuidad.
- Una rotacion planificada necesita primero una release puente firmada con la clave anterior que embeba la nueva publica.
- No cambiar el canal a releases firmadas con la nueva hasta que la poblacion objetivo instale el puente.
- Clientes offline durante la ventana no pueden saltar directamente a la clave nueva con el endpoint actual. Conservar una ruta manual al puente o implementar antes un servicio/canal para ambas cohortes.
- Conservar la clave anterior en respaldo restringido mientras haya clientes dependientes; no usarla para operacion ordinaria tras migrar.
- Si la privada fue comprometida, dejar de publicar con ella. Los clientes antiguos pueden requerir instalacion manual confiable; un puente firmado con una clave comprometida no recupera por si solo la confianza.

Toda rotacion requiere plan y prueba N-1 a N propios. No reemplazar silenciosamente la variable y los secrets esperando compatibilidad automatica.

## Firma Tauri y Authenticode

La `.sig` de Tauri autentica el artefacto para clientes que confian en la clave publica embebida. No identifica al editor ante Windows, no elimina SmartScreen y no sustituye Authenticode. Si se incorpora Authenticode, su certificado, timestamping y proteccion son un proceso adicional; las firmas Tauri siguen siendo necesarias.

## Proteccion obligatoria

- Proteger `main` con pull request, al menos una aprobacion y conversaciones resueltas.
- Exigir `Frontend and Rust (Linux)` y `Rust cfg(not(unix)) (Windows)`.
- Bloquear force-push y eliminacion de `main`.
- Aplicar una ruleset a `v*` que permita crear tags solo al rol autorizado y bloquee su actualizacion y eliminacion; nunca mover uno existente.
- Habilitar immutable releases de GitHub para impedir cambios en tags y assets despues de publicar.
- Proteger workflows y scripts de release mediante `CODEOWNERS` y revision obligatoria.
- Mantener el environment `release` con required reviewers, deployments limitados a `main` y los secrets de firma como environment secrets siempre que sea posible.

## Diagnostico

| Sintoma | Accion |
| --- | --- |
| `Invalid release tag` | Usar exactamente `vX.Y.Z`, sin prerelease ni espacios. |
| Versiones no coinciden | Igualar los tres manifests, regenerar `Cargo.lock` si corresponde y ejecutar `pnpm validate:version vX.Y.Z`. |
| Tag fuera de `main` | Integrar normalmente y crear un tag nuevo sobre `main`. |
| Falta la publica | Revisar `TAURI_UPDATER_PUBLIC_KEY`; no agregarla literal a la config base. |
| Falla la firma | Revisar ambos secrets, contrasena y pareja publica; no imprimir valores. |
| Se esperan archivos comprimidos | Son heredados; se exigen AppImage, DEB, RPM y NSIS nativos con sus `.sig`. |
| Falta una firma DEB/RPM | Cancelar; los cuatro canales updater deben estar firmados y verificados. |
| `latest.json` usa una clave generica | Cancelar; debe usar las cuatro claves especificas por formato. |
| `latest.json` apunta a MSI | Cancelar; MSI es exclusivamente manual. |
| El draft no aparece en `/latest` | Es normal; usar el procedimiento QA para staging. |
| No ofrece la primera version habilitada | Instalar manualmente la primera base con updater. |
| Windows cierra la app | Es esperado durante NSIS; despues verificar N y SQLite. |
| Linux queda en `Actualizacion instalada` | Confirmar el relanzamiento; no es automatico. |
| Rechazo tras rotar clave | Restaurar ruta al puente o instalar manualmente; no desactivar verificacion. |
| Falla despues de crear draft | Reintentar mismo tag/commit; eliminar el draft solo si se abandona. |

## Referencias

- [Workflow de release](../.github/workflows/release.yml)
- [Validador de version](../scripts/validate-version.mjs)
- [Generador y validador de `latest.json`](../scripts/latest-json.mjs)
- [Verificador de firmas](../scripts/verify-updater-signatures.mjs)
- [Plan de implementacion](plans/github-actions-updater.md)
