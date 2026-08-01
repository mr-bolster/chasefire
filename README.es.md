[English](README.md) · **Español**

# Chasefire

Persigue timecode, dispara cues.

Chasefire persigue timecode — **SMPTE LTC** por tarjeta de sonido, o **MTC** por
un puerto MIDI sin tarjeta de sonido ninguna —, vigila los valores que le hayas
programado y dispara **OSC, MIDI, MIDI Show Control y RTP-MIDI** justo en esos
momentos. Snapshots de mesa, cues de luces y clips de vídeo caen en el frame,
todas las noches, sin nadie conteniendo la respiración sobre un botón de GO.

También puede mandar el reloj de vuelta hacia fuera como **MIDI Time Code**, con
lo que la misma máquina se convierte en el conversor que le falta a un aparejo
con LTC por cable y un aparato que sólo entiende MTC.

Corre en la máquina que ya tienes: sin drivers de kernel, sin licencia que
caduque a mitad de gira, sin llamar a casa.

> **Estado: funciona, y no está terminado.** Captura en vivo, detección de
> frame rate, el motor de cues, OSC, MIDI, MSC, RTP-MIDI y salida de MTC están
> todos probados contra hardware real — un previo de verdad, un puerto MIDI de
> verdad y sockets que contestan. Queda: una entrada de control para que una
> superficie pueda armar el show, y timecode por Art-Net.

## Qué se ve

Una ventanita que dejas en una esquina. Cuatro cosas: si el show está armado,
la puerta a los ajustes, el timecode y Pablo.

Pablo es el guitarrista pequeño, y no es decoración. A las tres de la mañana en
una sala a oscuras nadie lee la palabra «enganchado», pero cualquiera nota de
reojo si el muñeco está tocando o dormido. Es un indicador de estado para la
visión periférica, que es el único tipo de atención que le sobra a un técnico.

| Pablo | Qué está pasando de verdad |
|---|---|
| Dormido, pompa de mocos, zzz | No llega timecode |
| Despierto pero en pijama y gorro | Timecode corriendo, **desarmado — no va a disparar nada** |
| Tocando, siguiendo el ritmo | Enganchado y armado |
| Tocando pero tiritando | Funciona, pero la señal está cerca del suelo |
| Tocando a trompicones, `?` encima | Señal perdida, contando por nuestra cuenta |

No puede mentir: hay un test que recorre todas las combinaciones de armado,
enganchado, freewheel y nivel de señal, y falla si la cara que pone alguna vez
contradice si una cue iba a salir de verdad.

No a todo el mundo le apetece un dibujo animado en la pantalla en el trabajo, así
que `--sober` lo cambia por los símbolos de transporte que el gremio ya lee sin
pensar —stop, pausa, play—, animados por los mismos cinco estados. La misma
información, las mismas reglas, sin muñeco. El símbolo en reposo es además el
icono de la aplicación.

Y cuando dispara una cue la ventana entera destella: verde si salió, **rojo si
no**, más largo y más fuerte, porque una cue que falló es lo único aquí por lo
que merece la pena interrumpir a alguien.

## Ponerlo en marcha

```bash
cargo build --release

# Ver qué puede escuchar esta máquina
./target/release/chasefire-cli devices

# La ventana, leyendo una tarjeta de sonido y disparando a un servidor de vídeo
./target/release/chasefire \
    --device "hw:CARD=CODEC,DEV=0" --channel 1 \
    --cues examples/resolume-columns.cues.json \
    --osc 192.168.1.50:7000
```

Se arma con el botón y sólo con el botón. No hay atajo de teclado a propósito:
la ventana está por encima de todo lo demás, así que puede robar el foco sin que
nadie se dé cuenta, y una tecla suelta que desarme el show en silencio es peor
problema que tener que apuntar a un botón.

## Dos idiomas

Inglés y español, se elige en Ajustes y se recuerda. No es una tabla de claves:
cada frase es un campo de una estructura que los dos idiomas tienen que
rellenar, así que una traducción que falte es un **error de compilación** y no
un hueco que alguien se encuentra en un escenario. Los errores de la propia
tarjeta también van traducidos, que son las palabras que se leen en el peor
momento posible.

## Probarlo sin hardware ninguno

```bash
# Escribir un WAV de LTC limpio
./target/release/chasefire-cli gen prueba.wav --fps 25 --seconds 25

# Decodificarlo y disparar las cues
./target/release/chasefire-cli wav prueba.wav --cues examples/resolume.cues.json

# O correr una lista de cues en tiempo real sin ninguna fuente de timecode
./target/release/chasefire-cli simulate --cues examples/resolume.cues.json

# Y medir lo que te cuesta tu propia tarjeta, con la salida en bucle a la entrada
./target/release/chasefire-cli latency --out-device "..." --device "..."
```

## Las reglas que importan

Comparar dos números lo hace cualquiera. Lo que separa una herramienta en la que
un técnico confía de una que apaga tras el primer bolo son los casos límite, así
que están escritos como tests en vez de descubiertos en el escenario.

- Una cue dispara cuando el timecode la **cruza**, no cuando coincide exacto —
  un frame perdido no puede comerse una cue en silencio.
- Un **salto grande es un seek, no un cruce.** Arrastra el playhead hasta los
  bises y las cues de en medio se quedan quietas en vez de dispararse todas.
- **Rebobinar re-arma**, porque eso es lo que significa «desde arriba».
  **Nada dispara hacia atrás**, y **arrancar a mitad de show no dispara nada.**
- Armar a mitad de show no vuelca todo lo que pasó mientras estaba apagado.
- LTC no tiene checksum, así que un frame corrupto decodifica a una hora
  equivocada pero verosímil. Los frames se comprueban en BCD, se comprueban
  contra el bit de paridad cuando la fuente lo mantiene, y **se retienen hasta
  que un segundo frame confirme cualquier salto**. Si no, un frame malo dispara
  una cue antes de tiempo y otra vez en su momento: un fallo, dos disparos, y
  nada en la lista de cues que lo explique después.
- Cuando cae la señal, **vuela en ciego** ocho frames antes de darse por
  vencido — la norma del gremio son de ocho a cuarenta.

Cada una de ellas es un test, y varias se escribieron después de que el código
demostrara que una suposición cómoda era falsa.

## Medido, no supuesto

Sobre un bucle analógico real — salida de tarjeta, cable, previo de micro,
conversor de entrada:

- Decodifica limpio desde **−53 dBFS de pico hasta saturar del todo**. Saturar
  no hace ningún daño: el bifase ya es prácticamente una onda cuadrada.
- **Subir el previo no compra nada.** Señal y ruido suben juntos; la relación
  señal-ruido se mantuvo dentro de 1 dB en ocho posiciones de ganancia. Con LTC
  quieres una señal limpia, no una señal fuerte.
- Los frames corruptos empiezan a aparecer por debajo de unos **12 dB de
  señal-ruido**. Por encima de 16 dB, ninguno. Ese umbral es al que está
  calibrado el medidor de nivel.
- Diciéndole el frame rate, engancha **al primer frame** — el suelo, porque un
  frame son 80 bits y la palabra de sincronismo son los últimos 16.
  Averiguándolo él solo, tres frames.

## Cómo está repartido

```
crates/ltc      Decodificador y codificador de SMPTE LTC. DSP puro, sin E/S.
crates/chase    Decide de qué frames decodificados fiarse.
crates/cue      La tabla de cues y las reglas de disparo. Sin sockets.
crates/audio    Captura y generación en vivo, decodificando en el callback.
crates/sink     Por dónde sale una cue: OSC, MIDI, MSC, RTP-MIDI, MTC.
crates/rtpmidi  Sesiones RTP-MIDI (AppleMIDI), habladas aquí y no delegadas.
crates/pablo    El guitarrista pequeño, y la regla de que no puede mentir.
crates/show     Todo lo anterior, cableado en un solo sitio.
apps/chasefire       La ventana.
apps/chasefire-cli   Línea de comandos, simulador y herramientas de medida.
tools/               Montar la hoja de sprites a partir de las tiras.
```

## Compilar

```bash
cargo test          # no hace falta hardware
cargo build --release
```

En Linux necesitas las cabeceras de ALSA: `sudo apt install libasound2-dev`.

## Licencia

Dos, a propósito, y la línea entre ellas no es arbitraria.

**El motor es MPL-2.0** — `ltc`, `cue`, `chase`, `audio`, `sink`, `rtpmidi`,
`show`. Eso es el decodificador, las reglas de disparo, el chaser y las salidas:
las partes donde viven los casos límite y donde acertar importa. La MPL es
copyleft a nivel de fichero, así que las mejoras a esos ficheros siguen abiertas
y las puede usar cualquier cosa, incluido software que no sea abierto en
absoluto.

**El programa es GPL-3.0-or-later** — todo lo que hay bajo `apps/`, y la crate
`pablo`, que lleva los dibujos.

Pablo y los símbolos de transporte los dibujó Claude a partir de un guion,
ejemplos y correcciones de Leo Bolster. Dicho claro aquí porque una pantalla de
créditos que insinúa que dibujó una persona lo que dibujó una máquina es una
mentira pequeña, y este proyecto no necesita ninguna.

El código es abierto y lo seguirá siendo. Lo que se paga son los binarios
firmados y listos para usar — una vez, no todos los años.

### Sobre los parches

Por favor, **abre un issue en vez de un pull request.** No por antipatía: el
código fusionado pertenece a quien lo escribió, y un puñado de líneas aceptadas
pueden impedir para siempre que el autor licencie su propio trabajo de otra
manera más adelante. Describe el problema, o el arreglo, y se escribirá aquí
dándote crédito en el commit.
