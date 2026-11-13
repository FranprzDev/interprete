# Intérprete

Traductor simultáneo **español <-> inglés** para Windows, gratis y local. Hablás en español y sale la traducción al inglés (en pantalla y en voz). Alguien habla en inglés y ves la traducción al español en tiempo real.

Pensado para charlas y llamadas (Zoom, Meet). No manda tu audio a ningún servidor: todo corre en tu PC.

## Instalar (Windows)

1. Bajá `Interprete-windows-x64.zip` de [Releases](../../releases/latest).
2. Descomprimilo y abrí `Interprete.exe`. Si Windows avisa "protegió su PC", tocá **Más información** y **Ejecutar de todas formas** (el programa no está firmado).
3. La primera vez tocá **Instalar**. Descarga unos 1,7 GB (motores y modelos) una sola vez, con barra de progreso.
4. Cuando diga listo, marcá lo que necesites:
   - **Yo hablo (español a inglés)**: elegí tu micrófono. Opcional: "Decir la traducción en voz alta".
   - **Escucho (inglés a español)**: capta el audio de la PC (lo que suena en Zoom, Meet o un video) y muestra la traducción en español.

## Usarlo en una llamada (que el otro te escuche en inglés)

Instalá un cable de audio virtual gratis (por ejemplo VB-Cable). En Intérprete tildá "Decir la traducción en voz alta" y en **Salida** elegí `CABLE Input`. En Zoom o Meet elegí `CABLE Output` como micrófono.

## Requisitos

Windows 10 u 11 de 64 bits, 8 GB de RAM y unos 3 GB libres. Funciona sin placa de video; en PCs lentas la traducción tarda unos segundos por frase.

## Cómo está hecho

Rust. Captura (cpal) -> detector de pausas -> `whisper.cpp` (voz a texto) -> `llama.cpp` con Qwen2.5 1.5B (traducción) -> voz del sistema. Cada etapa es un `trait` (`Stt`, `Translator`, `Tts` en `core/src`), así se puede cambiar el motor, por ejemplo por un modelo Liquid LFM servido con llama.cpp.

Pruebas por consola (necesita los motores instalados):

```
interprete install
interprete translate-file es en frase.wav
```

El CI corre un test de punta a punta: genera audio real en español e inglés, lo pasa por whisper y el traductor y verifica el resultado.

Licencia MIT.

## Versión web (sin instalar)

Abrí https://interprete-traductor.franprzdev.workers.dev en Chrome o Edge, tocá **Empezar** (la primera vez baja unos 200 MB que quedan guardados) y después **Empezar a escuchar**. Corre en el navegador (whisper y traducción por WebGPU/WASM), sin servidor y sin cuenta. Código en `web/`, hosting en Cloudflare con `wrangler deploy`.
