# Plan de GitHub Actions y actualizaciones

## Objetivo

Automatizar la validacion, compilacion y publicacion de Linea POS para Linux y Windows mediante GitHub Actions, y agregar actualizaciones firmadas con el plugin oficial de Tauri.

La aplicacion consultara actualizaciones al iniciar. Cuando exista una version nueva, mostrara una modal y preguntara al usuario si desea actualizar.

## Decisiones del proyecto

- El repositorio `98beto/pos_tauri` y las GitHub Releases seran publicos; la rama principal es `main`.
- La primera version automatizada soportara Linux y Windows x86_64.
- Linux seguira generando AppImage, `.deb` y `.rpm`.
- AppImage, `.deb` y `.rpm` seran canales autoactualizables independientes en Linux.
- Windows generara NSIS `.exe` y MSI `.msi`.
- NSIS sera el canal autoactualizable en Windows.
- MSI se conservara como instalador manual.
- La aplicacion preguntara antes de descargar e instalar.
- No se descargaran ni instalaran actualizaciones silenciosamente.
- GitHub Releases hospedara instaladores, firmas y `latest.json`.
- Se mantendra estable el identificador `com.beto98.pos-tauri` para conservar la ruta de datos.

## Artefactos esperados

### Linux

Instalacion manual:

- `*.AppImage`
- `*.deb`
- `*.rpm`

Updater:

- `*.AppImage` y `*.AppImage.sig`
- `*.deb` y `*.deb.sig`
- `*.rpm` y `*.rpm.sig`

El updater oficial de Tauri reemplaza AppImage, instala DEB mediante `dpkg -i` y RPM mediante `rpm -U`. DEB y RPM pueden requerir elevacion administrativa.

### Windows

Instalacion manual:

- NSIS `*-setup.exe`
- MSI `*.msi`

Updater:

- `*-setup.exe`
- `*-setup.exe.sig`

El manifiesto `latest.json` debe apuntar al artefacto NSIS para `windows-x86_64-nsis`, no al MSI.

### Metadatos

- `latest.json` con version, notas, fecha, URL y firma por formato.
- Claves exactas `linux-x86_64-appimage`, `linux-x86_64-deb`, `linux-x86_64-rpm` y `windows-x86_64-nsis`.
- El conjunto publicado contiene exactamente nueve artefactos nativos y `latest.json`; agregar otros assets requiere actualizar y volver a validar ese contrato.

## Etapa 1: Preparar el repositorio

- Inicializar o conectar el proyecto con un repositorio GitHub.
- Definir la rama principal.
- Crear `.github/workflows/`.
- Agregar `*.tsbuildinfo` a `.gitignore`.
- Reemplazar metadatos genericos de `src-tauri/Cargo.toml`.
- Confirmar el propietario y nombre del repositorio para formar el endpoint:

```text
https://github.com/98beto/pos_tauri/releases/latest/download/latest.json
```

### Resultado esperado

El proyecto queda listo para ejecutar CI y publicar releases desde tags.

## Etapa 2: Fijar toolchains

- Fijar Node.js 24.15.0 para CI.
- Fijar pnpm 12.4.1 y declararlo como `packageManager` en `package.json`.
- Agregar `rust-toolchain.toml` con Rust 1.98.1, `rustfmt` y `clippy`.
- Usar `pnpm install --frozen-lockfile`.
- Usar Cargo con `--locked`.
- Configurar cache de pnpm y Rust en GitHub Actions.

### Resultado esperado

Los builds locales y remotos usan versiones previsibles de las herramientas.

## Etapa 3: Validar versionado

La version existe actualmente en:

- `package.json`
- `src-tauri/Cargo.toml`
- `src-tauri/tauri.conf.json`

Agregar una verificacion que:

- Extraiga la version del tag `vX.Y.Z`.
- Compruebe que coincida en los tres archivos.
- Rechace tags con formato invalido.
- Rechace una version ya publicada.
- Detenga la release si los lockfiles cambian durante la instalacion.

### Resultado esperado

El tag, los instaladores y `latest.json` siempre publican la misma version.

## Etapa 4: Integrar el updater de Tauri

Agregar dependencias frontend:

```text
@tauri-apps/plugin-updater
@tauri-apps/plugin-process
```

Agregar dependencias Rust:

```text
tauri-plugin-updater
tauri-plugin-process
```

Registrar Process en el builder y registrar Updater solamente cuando `plugins.updater` sea un objeto de configuracion valido:

- Updater para comprobar, descargar, verificar e instalar.
- Process para relanzar la aplicacion despues de instalar.

La configuracion base no contiene updater. En `tauri dev` el backend no registra el plugin y el frontend no consulta actualizaciones ni muestra la accion manual. Los builds de release reciben la configuracion mediante el overlay efimero.

No usar `plugin-upload`; ese plugin solo transfiere archivos por HTTP y no implementa actualizaciones de aplicaciones.

### Resultado esperado

La aplicacion puede consumir actualizaciones firmadas mediante las APIs oficiales.

## Etapa 5: Configurar permisos minimos

Actualizar `src-tauri/capabilities/default.json` con los permisos exactos generados por las versiones instaladas, equivalentes a:

```text
updater:allow-check
updater:allow-download-and-install
process:allow-restart
```

No conceder permisos globales que no utilice la aplicacion.

### Resultado esperado

El frontend puede comprobar, instalar y reiniciar sin ampliar innecesariamente la superficie de permisos.

## Etapa 6: Generar y proteger las claves

Generar una clave de firma Tauri una sola vez fuera del repositorio.

Guardar en GitHub Secrets:

```text
TAURI_SIGNING_PRIVATE_KEY
TAURI_SIGNING_PRIVATE_KEY_PASSWORD
```

Guardar la clave publica como variable de repositorio `TAURI_UPDATER_PUBLIC_KEY` e inyectarla mediante un overlay efimero de configuracion durante la release.

Reglas:

- No guardar la clave privada en Git.
- No guardar la clave privada en archivos `.env`.
- No imprimir la clave en logs de Actions.
- Mantener un respaldo seguro fuera de GitHub.
- No regenerar la clave para cada release.

La firma del updater no sustituye una futura firma Authenticode de Windows.

### Resultado esperado

Las instalaciones solo aceptan actualizaciones creadas por el proyecto.

## Etapa 7: Configurar artefactos updater

Crear durante la release un overlay efimero equivalente a esta configuracion; el mismo overlay agrega `pubkey` desde `TAURI_UPDATER_PUBLIC_KEY` sin escribirla en la configuracion base:

```json
{
  "bundle": {
    "active": true,
    "createUpdaterArtifacts": false
  },
  "plugins": {
    "updater": {
      "endpoints": [
        "https://github.com/98beto/pos_tauri/releases/latest/download/latest.json"
      ]
    }
  }
}
```

Configurar bundles explicitos por job:

- Linux: `appimage,deb,rpm`.
- Windows updater: `nsis`.
- Windows manual: `msi` en un paso o job separado para evitar ambiguedad en `latest.json`.

### Resultado esperado

Linux sanea primero el AppImage y firma despues los bytes finales de AppImage, DEB y RPM. NSIS genera su firma en el build dedicado; MSI permanece manual.

## Etapa 8: Implementar la experiencia de actualizacion

Crear una capa frontend dedicada, por ejemplo:

- `src/api/updater.ts`
- `src/features/updater/UpdateController.tsx`
- `src/features/updater/UpdateController.test.tsx`

Montar `UpdateController` a nivel global para que funcione tanto en login como dentro del POS.

### Flujo al iniciar

1. La aplicacion inicia normalmente.
2. El controlador consulta silenciosamente si existe una actualizacion.
3. Si no existe, no muestra ningun elemento.
4. Si la consulta falla por red, no bloquea el inicio.
5. Si existe una version nueva, abre una modal accesible.
6. La modal muestra version actual, version nueva y notas.
7. La modal ofrece `Actualizar ahora` y `Despues`.
8. `Despues` cierra la modal y no vuelve a mostrarla durante esa sesion.
9. En el siguiente inicio se vuelve a consultar y preguntar.
10. `Actualizar ahora` inicia la descarga y muestra progreso.
11. Durante la instalacion la modal no se puede cerrar.
12. En Windows el instalador puede cerrar la aplicacion antes de que la llamada regrese.
13. En Linux, si la llamada regresa, se muestra `Reiniciar y completar actualizacion` y la aplicacion solo se relanza cuando el usuario confirma.

### Advertencias operativas

- Informar que el reinicio cerrara la sesion.
- Informar que el carrito no cobrado se encuentra en memoria y se perdera.
- No instalar automaticamente mientras exista una operacion de checkout en curso.
- No ocultar errores de firma como simples errores de red.
- No mostrar detalles internos del sistema o del plugin.

### Estado visual sugerido

```text
idle
checking
available
downloading
installing
readyToRestart
error
```

Reutilizar `Modal`, `Button`, `Spinner` y `Toast` existentes.

Agregar una accion manual `Buscar actualizaciones` dentro del shell para recuperar el flujo si el usuario eligio `Despues`. No mostrarla en desarrollo, donde el updater esta deshabilitado.

### Resultado esperado

Las actualizaciones nunca interrumpen silenciosamente al usuario y siempre requieren confirmacion.

## Etapa 9: Probar el controlador updater

Simular `@tauri-apps/plugin-updater` y `@tauri-apps/plugin-process` en Vitest.

Cubrir:

- Una sola comprobacion bajo React StrictMode.
- Aplicacion ya actualizada.
- Nueva version disponible.
- Version y notas correctas.
- Botones `Actualizar ahora` y `Despues`.
- No descargar antes de la confirmacion.
- Progreso con y sin tamano conocido.
- Error de red recuperable.
- Firma invalida.
- Descarga interrumpida.
- Modal no cerrable durante instalacion.
- No relanzar antes de finalizar en plataformas donde el instalador devuelve el control.
- En Linux, relanzamiento solo tras confirmacion si la instalacion devuelve el control; en Windows, NSIS puede cerrar la aplicacion.
- Advertencia de sesion y carrito.
- Consulta manual despues de elegir `Despues`.
- En desarrollo no se llama al plugin ni se muestra la consulta manual.
- En produccion se conservan la comprobacion inicial y manual.

### Resultado esperado

El flujo del updater queda cubierto sin realizar descargas reales durante las pruebas.

## Etapa 10: Crear workflow de CI

Crear `.github/workflows/ci.yml`.

Disparadores:

```text
pull_request
push a la rama principal
```

Comandos minimos:

```bash
pnpm install --frozen-lockfile
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
```

Ejecutar la validacion principal en Ubuntu y agregar al menos una comprobacion Rust en Windows para cubrir ramas `cfg(not(unix))`.

### Resultado esperado

Los cambios no pueden integrarse si frontend, Rust, formato, lint o tests fallan.

## Etapa 11: Crear workflow de release

Crear `.github/workflows/release.yml`.

Disparadores:

```text
workflow_run desde la solicitud no privilegiada de tags
workflow_dispatch controlado con modos validate y publish
```

Permisos:

```yaml
permissions:
  contents: read
```

Conceder `contents: write` solamente al job final de publicacion.

Agregar concurrencia global de publicacion para impedir que releases de tags distintos compitan por el canal `latest`.

### Job de validacion

- Validar tag y versiones.
- Instalar con lockfiles congelados.
- Ejecutar pruebas frontend y Rust.

### Job Linux

- Runner `ubuntu-24.04`.
- Instalar dependencias oficiales de Tauri 2.
- Generar AppImage, `.deb` y `.rpm`.
- Eliminar del AppImage final las familias `libwayland-client`, `cursor`, `egl` y `server` incluidas por el bundler.
- Firmar explicitamente los bytes finales de AppImage, DEB y RPM.
- Verificar las tres firmas con `minisign` antes de subir exactamente seis archivos.

El saneamiento evita mezclar las bibliotecas Wayland incluidas por Ubuntu con Mesa reciente del host. La firma anterior al saneamiento se descarta siempre.

### Job Windows NSIS

- Runner `windows-latest`.
- Generar el instalador NSIS sin exponer la clave privada al build.
- Firmar despues el `-setup.exe` final en un paso minimo y generar `-setup.exe.sig`.
- Subir el resultado como artefacto temporal de Actions para el job final.

### Job Windows MSI

- Generar MSI como instalador manual.
- Compilarlo sin `VITE_UPDATER_ENABLED` y sin `plugins.updater`.
- Subirlo como artefacto temporal de Actions para el job final.
- Evitar que sustituya la entrada NSIS en `latest.json`.

### Ensamblado y publicacion

- Usar una version estable de `tauri-apps/tauri-action`, preferentemente fijada por SHA.
- Ejecutar siempre un job `assemble` con `contents: read`, verificar las cuatro firmas y generar `latest.json` a partir de los artefactos nativos.
- En `mode=validate`, subir el candidato verificado como artifact de Actions y omitir por completo el job de publicacion.
- Crear o reutilizar la release en borrador despues de que termine la matriz y se verifiquen las firmas locales.
- Conceder `contents: write` solo al job `publish`, condicionado a un tag valido.
- Publicar automaticamente y de forma atomica solamente cuando los nueve assets, `latest.json`, firmas y SHA-256 remotos hayan sido revalidados.
- Si falla una plataforma, no crear un borrador nuevo; si existia uno de un intento anterior, conservarlo, y mantener la release estable anterior como `latest`.

### Resultado esperado

Un tag valido produce una release completa para Linux y Windows sin publicar manifiestos parciales.

## Etapa 12: Verificar `latest.json`

Antes de publicar comprobar:

- Version igual al tag.
- Entradas `linux-x86_64-appimage`, `linux-x86_64-deb`, `linux-x86_64-rpm` y `windows-x86_64-nsis` apuntando a su propio formato.
- Cuatro firmas no vacias y criptograficamente correspondientes.
- URLs pertenecientes a la misma release.
- Archivos referenciados realmente existentes.
- Ausencia de referencias a MSI como canal updater.

### Resultado esperado

Los clientes siempre descargan el formato correcto y una firma correspondiente.

## Etapa 13: Prueba real N-1 a N

1. Instalar una release N-1 real en Linux mediante AppImage, DEB y RPM.
2. Instalar una release N-1 real en Windows mediante NSIS.
3. Crear propietario, productos, inventario y ventas.
4. Publicar N mediante el workflow estable, o usar una build QA separada con un endpoint publico/controlado; los drafts no son visibles mediante `/releases/latest`.
5. Confirmar que N-1 detecta N al iniciar.
6. Elegir `Despues` y comprobar que la aplicacion sigue funcionando.
7. Reiniciar y confirmar que vuelve a preguntar.
8. Elegir `Actualizar ahora`.
9. Verificar descarga, firma e instalacion; confirmar reemplazo AppImage, elevacion DEB/RPM y cierre esperado de NSIS.
10. Confirmar que SQLite y los datos sobreviven.
11. Probar firma invalida, JSON corrupto y descarga interrumpida.
12. Probar rutas con espacios y usuario sin privilegios administrativos.

### Resultado esperado

La actualizacion funciona de extremo a extremo y conserva los datos persistidos.

## Etapa 14: Documentar operacion de releases

La guia operativa se mantiene en [`docs/releases.md`](../releases.md).

Documentar:

- Como incrementar la version.
- Como crear y enviar el tag.
- Como revisar una release en borrador.
- Como verificar firmas y `latest.json`.
- Como publicar o cancelar una release.
- Como recuperar una release fallida.
- Como respaldar y rotar claves sin bloquear clientes antiguos.
- Comportamiento de AppImage, DEB, RPM y NSIS como canales independientes, y MSI como instalador manual.

### Resultado esperado

Una release puede publicarse de forma repetible sin depender de conocimiento informal.

## Comandos de verificacion

```bash
pnpm install --frozen-lockfile
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml --release
```

En CI tambien se verificara la generacion de:

- AppImage.
- `.deb`.
- `.rpm`.
- NSIS `.exe`.
- MSI.
- Artefactos updater firmados.
- `latest.json`.

## Criterios de finalizacion

- CI se ejecuta en pushes y pull requests.
- Los tags `vX.Y.Z` crean o reutilizan un borrador durante el ensamblado final y lo publican automaticamente despues de todos los gates.
- Linux genera AppImage, `.deb` y `.rpm`.
- Windows genera NSIS y MSI.
- AppImage, DEB, RPM y NSIS cuentan con firmas updater verificadas.
- `latest.json` contiene solamente cuatro claves especificas por formato.
- `mode=validate` construye y verifica sin permisos de publicacion.
- `latest.json` solo se publica cuando todas las plataformas terminan.
- La aplicacion consulta actualizaciones al iniciar.
- La modal solo aparece cuando hay una version nueva.
- El usuario puede elegir `Actualizar ahora` o `Despues`.
- No existe descarga o instalacion sin consentimiento.
- En Linux el relanzamiento requiere confirmacion si la instalacion devuelve el control; Windows puede cerrarse durante NSIS.
- Los datos SQLite sobreviven a la actualizacion.
- La clave privada nunca aparece en el repositorio ni en logs.
- Una release incompleta no sustituye la version estable anterior.
