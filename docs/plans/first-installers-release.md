# Plan para generar los primeros instaladores

## Objetivo

Corregir las advertencias de Actions que todavia usan Node.js 20, configurar la firma del updater y publicar la primera release `v0.1.0` de Linea POS para Linux y Windows.

La primera release debe producir instaladores manuales, artefactos firmados para el updater y un `latest.json` valido. La prueba automatica N-1 a N se realizara posteriormente con `v0.1.0` instalada y una nueva version `v0.1.1`.

## Estado actual

- El repositorio publico es `98beto/pos_tauri`.
- La rama principal es `main`.
- CI ya se ejecuto correctamente en Linux y Windows.
- No existen tags, releases ni artifacts publicados.
- La version actual es `0.1.0` en los tres archivos requeridos.
- El identificador `com.beto98.pos-tauri` debe permanecer sin cambios para conservar los datos de la aplicacion.
- CI no genera instaladores; los instaladores se crean exclusivamente mediante el workflow `Release`.

## Etapa 1: Corregir advertencias de Node.js 20

Las advertencias deben corregirse antes de publicar. GitHub actualmente fuerza las Actions antiguas a ejecutarse con Node.js 24, pero esa compatibilidad es temporal.

Actualizar todas las apariciones correspondientes en `.github/workflows/ci.yml` y `.github/workflows/release.yml`:

| Action | Version | SHA fijado |
| --- | --- | --- |
| `actions/checkout` | `v5.1.0` | `fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09` |
| `actions/setup-node` | `v5.0.0` | `a0853c24544627f65ddf259abe73b1d18a591444` |
| `pnpm/action-setup` | `v5.0.0` | `fc06bc1257f339d1d5d8b3a19a8cae5388b55320` |
| `actions/upload-artifact` | `v7.0.1` | `043fb46d1a93c77aae656e7c1c64a875d1fc6a0a` |
| `actions/download-artifact` | `v7.0.0` | `37930b1c2abaa49bbe596cd826c3c89aef350131` |

`Swatinem/rust-cache` y `tauri-apps/tauri-action` ya usan Node.js 24 y no requieren cambios.

Actualizar `scripts/release-workflow.node-test.mjs` para:

- Exigir los nuevos SHA.
- Rechazar los SHA anteriores basados en Node.js 20.
- Mantener la regla que obliga a fijar todas las Actions mediante un SHA completo de 40 caracteres.

### Resultado esperado

CI y Release dejan de emitir advertencias por Actions basadas en Node.js 20.

## Etapa 2: Verificar la correccion

Ejecutar localmente:

```bash
pnpm install --frozen-lockfile
pnpm test:release
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml --release
```

Validar los workflows con `actionlint` cuando este disponible.

Confirmar que los lockfiles no cambiaron inesperadamente:

```bash
git diff --exit-code -- pnpm-lock.yaml src-tauri/Cargo.lock
```

### Resultado esperado

Las pruebas, compilaciones, formato, Clippy y validaciones de workflows terminan correctamente.

## Etapa 3: Publicar la correccion en `main`

Crear un commit dedicado a la actualizacion de Actions y enviarlo al remoto correcto:

```text
git@github.com:98beto/pos_tauri.git
```

No usar `98beto/tauri_pos.git`; ese no es el repositorio configurado para el updater.

Esperar a que CI finalice correctamente y comprobar que ya no aparezcan advertencias de Node.js 20.

### Resultado esperado

La rama `main` contiene los workflows corregidos y CI esta en verde.

## Etapa 4: Generar la clave del updater

Generar una unica pareja de claves fuera del repositorio:

```bash
pnpm tauri signer generate -w ~/.tauri/linea-pos-updater.key
```

Usar el prompt interactivo para definir una contrasena robusta.

Reglas obligatorias:

- No pasar la contrasena como argumento del comando.
- No guardar la clave privada dentro del proyecto.
- No guardar la clave privada en archivos `.env`.
- No pegar claves en commits, issues, logs o notas de release.
- Conservar al menos dos respaldos cifrados e independientes.
- Probar que uno de los respaldos puede restaurarse.
- No regenerar la clave para cada release.

### Resultado esperado

Existe una clave privada protegida fuera del repositorio y su clave publica asociada.

## Etapa 5: Configurar GitHub

En `Settings > Secrets and variables > Actions`, crear la variable de repositorio:

```text
TAURI_UPDATER_PUBLIC_KEY
```

El valor debe ser el contenido completo de la clave publica generada por Tauri.

En `Settings > Environments`, crear el environment `release` con:

- Required reviewers.
- Deployments limitados a `main`.
- Secret `TAURI_SIGNING_PRIVATE_KEY` con el contenido completo de la clave privada.
- Secret `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` con la contrasena.

Configurar tambien:

- Una ruleset que impida modificar o eliminar tags `v*`.
- Immutable releases.
- Permisos de Actions que permitan al job `publish` usar `contents: write`.

Introducir los valores desde la interfaz protegida de GitHub. No incluir secretos en comandos de terminal porque pueden quedar en el historial o en la lista de procesos.

### Resultado esperado

Los jobs firmados pueden acceder a las credenciales solamente despues de la aprobacion del environment `release`.

## Etapa 6: Preparar `v0.1.0`

Confirmar que `0.1.0` coincide en:

- `package.json`.
- `src-tauri/Cargo.toml`.
- `src-tauri/tauri.conf.json`.

Ejecutar:

```bash
pnpm validate:version v0.1.0
```

Revisar que todos los cambios necesarios se encuentren confirmados en `main` y que CI este en verde.

### Resultado esperado

El tag, Cargo, Tauri y el frontend publicaran exactamente la version `0.1.0`.

## Etapa 7: Crear y enviar el tag

Crear el tag anotado sobre el commit validado de `main`:

```bash
git switch main
git pull --ff-only origin main
git tag -a v0.1.0 -m "Linea POS v0.1.0"
git push origin v0.1.0
```

No mover, eliminar ni reutilizar el tag despues de enviarlo.

### Resultado esperado

El push ejecuta primero `Release request` y despues el workflow privilegiado `Release` cargado desde `main`.

## Etapa 8: Aprobar y supervisar la release

El workflow debe ejecutar esta secuencia:

1. Validar el tag, las versiones, los lockfiles y las pruebas.
2. Generar el MSI manual de Windows.
3. Esperar la aprobacion del environment para los jobs firmados.
4. Generar AppImage, `.deb` y `.rpm` en Ubuntu.
5. Generar NSIS en Windows.
6. Verificar criptograficamente las firmas.
7. Crear o reutilizar una release en borrador.
8. Generar y validar un unico `latest.json`.
9. Subir y volver a descargar los assets para verificarlos.
10. Publicar la release solamente cuando todo este completo.

Si un job falla, no publicar manualmente el borrador parcial. Corregir la configuracion y volver a ejecutar el workflow.

### Resultado esperado

Una release incompleta permanece como borrador y no sustituye la version estable anterior.

## Etapa 9: Verificar los instaladores publicados

La release debe contener estos ocho archivos:

```text
*.AppImage
*.AppImage.sig
*.deb
*.rpm
*-setup.exe
*-setup.exe.sig
*.msi
latest.json
```

Canales del updater:

- Linux usa el AppImage.
- Windows usa el instalador NSIS `*-setup.exe`.

Instaladores manuales:

- Linux conserva `.deb` y `.rpm`.
- Windows conserva `.msi`.

Comprobar la release en:

```text
https://github.com/98beto/pos_tauri/releases
```

Comprobar el manifiesto estable en:

```text
https://github.com/98beto/pos_tauri/releases/latest/download/latest.json
```

Validar que:

- La version sea `0.1.0`.
- `linux-x86_64` apunte al AppImage.
- `windows-x86_64` apunte al NSIS y nunca al MSI.
- Las firmas no esten vacias.
- Todas las URLs pertenezcan a `v0.1.0`.
- Todos los archivos referenciados existan.

### Resultado esperado

Los usuarios pueden descargar instaladores Linux y Windows, y las instalaciones AppImage/NSIS quedan preparadas para recibir actualizaciones firmadas.

## Etapa 10: Probar una actualizacion real

`v0.1.0` sera la primera version que contiene el updater, por lo que no puede demostrar por si sola el flujo N-1 a N.

Despues de instalar y probar `v0.1.0`:

1. Preparar `v0.1.1` actualizando las tres versiones.
2. Publicar `v0.1.1` mediante el mismo proceso.
3. Abrir la instalacion AppImage `v0.1.0` y confirmar que detecta `v0.1.1`.
4. Repetir con una instalacion NSIS `v0.1.0` en Windows.
5. Probar `Despues`, descarga, firma, instalacion y reinicio.
6. Confirmar que SQLite, propietario, productos, inventario y ventas sobreviven.
7. Probar rutas con espacios y usuarios sin privilegios administrativos.
8. Probar firma invalida, manifiesto corrupto y descarga interrumpida en un entorno controlado.

### Resultado esperado

La actualizacion N-1 a N funciona en Linux y Windows sin perder datos persistidos.

## Criterios de finalizacion

- CI no muestra advertencias de Actions basadas en Node.js 20.
- Todos los Actions permanecen fijados por SHA completo.
- La clave privada nunca aparece en el repositorio ni en logs.
- El tag `v0.1.0` produce una release completa.
- Linux publica AppImage, `.deb` y `.rpm`.
- Windows publica NSIS y MSI.
- AppImage y NSIS cuentan con firmas validas.
- `latest.json` apunta exclusivamente a AppImage y NSIS.
- Una release parcial nunca se publica como estable.
- La segunda release permite completar una prueba real N-1 a N.
