# Icono de Linea POS

El icono aplicado es exactamente el concepto 7: la L continua, gruesa y redondeada del concepto 2, combinada con el fondo petróleo, el arco abierto y el punto turquesa del concepto 6. Conserva la geometría y los colores definidos en `docs/icon-concepts/07-linea-cierre.svg`; sólo cambia el tamaño de salida a 1024 × 1024 (viewBox 512 × 512). La lámina de seis conceptos y la imagen de referencia `Imagen de ChatGPT 4 nov 2026, 08_11_17 p.m.png` se conservan sin cambios.

Para regenerar los iconos de Tauri desde el SVG fuente, usa un directorio temporal. El CLI también genera iconos móviles; copia sólo los formatos desktop que ya existen en `src-tauri/icons/`:

```sh
pnpm tauri icon public/app-icon.svg --output /tmp/opencode/linea-approved7
cp /tmp/opencode/linea-approved7/{32x32.png,128x128.png,128x128@2x.png,icon.png,icon.icns,icon.ico,Square30x30Logo.png,Square44x44Logo.png,Square71x71Logo.png,Square89x89Logo.png,Square107x107Logo.png,Square142x142Logo.png,Square150x150Logo.png,Square284x284Logo.png,Square310x310Logo.png,StoreLogo.png} src-tauri/icons/
```
