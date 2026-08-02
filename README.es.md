[English](README.md) · **Español**

# Chasefire

Persigue timecode, dispara cues.

Chasefire sigue timecode — **SMPTE LTC** por tarjeta de sonido, o **MTC** por un
puerto MIDI sin tarjeta ninguna — y en los momentos que le programes dispara
**OSC, MIDI, MIDI Show Control** y **RTP-MIDI**. Snapshots de mesa, cues de
luces y clips de vídeo caen en el frame, todas las noches, sin nadie
conteniendo la respiración sobre un botón de GO.

También puede mandar el reloj de vuelta hacia fuera como **MTC**, con lo que la
misma máquina es el conversor entre un aparejo con LTC por cable y un aparato
que sólo entiende MTC.

## El hueco que tapa

Software que *convierte* timecode hay de sobra, y show controllers que
reproducen media también. De lo que no hay producto es de la caja de en medio:
**algo que siga timecode y dispare a todo lo demás, sin pretender ser un
servidor de media.**

Hoy ese trabajo se hace encadenando dos aplicaciones —un conversor y una
superficie de control— o montándotelo tú con un toolkit. Cada eslabón de más es
otra cosa que arrancar, otro reloj, y otro sitio por donde se cae el show.

| | |
|---|---|
| Conversores gratuitos (TXL20 y compañía) | convierten timecode; no disparan cues |
| TimeLord | reproduce media y genera timecode |
| Show Cue System | un show controller completo para Windows |
| QLab | el que todo el mundo quiere — sólo macOS |
| Chataigne | un toolkit: capaz de todo, y te lo montas tú |
| **Chasefire** | sigue timecode, dispara a todo, y no hace nada más |

## Con qué habla

Eliges un preset y te escribe una cue que funciona, sacada de la documentación
de cada fabricante:

**Resolume · QLab · grandMA3 · grandMA2 (por MSC) · ChamSys MagicQ ·
Behringer X32/M32 · Behringer Wing · Waves SuperRack**

Una cue es una **lista de mensajes, cada uno con su destino**, porque un momento
del show no es un cable: la cue que arranca el vídeo también cambia un snapshot
en la mesa. QLab no quiere argumento ninguno, grandMA3 quiere una línea de
comandos entera como string, y una Behringer Wing necesita dos mensajes en el
orden correcto. Todo eso se puede escribir.

## Qué tiene

- **Entrada:** LTC por tarjeta de sonido a 44,1 / 48 / 96 kHz, o MTC por puerto
  MIDI. 23,98, 24, 25, 29,97 drop-frame y 30 fps. 50/60 quedan desactivados
  hasta demostrar su representación por pares contra un fixture real e independiente.
- **Salida:** OSC, MIDI, MSC, RTP-MIDI — varios destinos a la vez, cada uno con
  un nombre al que una cue puede apuntar.
- **Reloj de salida:** MTC, con el ritmo correcto, para que un receptor pueda
  engancharse.
- **Offset** en frames, para compensar el retardo de la tarjeta, la red y el
  otro extremo. **Freewheel** lo que le digas.
- Una ventanita para dejar en una esquina, en **español o inglés**.
- Las listas de cues son ficheros JSON normales: se leen, se comparan y se
  mandan por correo.

## Descargarlo

**[Descargas](https://github.com/mr-bolster/chasefire/releases)** — Windows y
Linux, nada que instalar.

Windows va a avisar de que el editor es desconocido: estos binarios todavía no
están firmados. *Más información* → *Ejecutar de todas formas*.

## Apoyarlo

**Aquí no se paga nada.** Ni el programa, ni los binarios, ni una
actualización, ni el año que viene. No hay clave de licencia, ni periodo de
prueba, ni caducidad, ni nada apagado si no pagas nunca.

Funciona por sistema de honor. Si Chasefire te da de comer, hay un botón de
**Donar** en Ajustes — paga lo que te parezca que valió, una vez, cuando te
apetezca. Ése es todo el acuerdo.

## Compilarlo tú

```bash
cargo test          # no hace falta hardware
cargo build --release
```

En Linux necesitas las cabeceras de ALSA: `sudo apt install libasound2-dev`.

Los edge cases que el motor de cues resuelve bien, y los números medidos sobre
hardware real en vez de supuestos, están en
[`docs/how-it-works.md`](docs/how-it-works.md).

## Licencia

**El motor es MPL-2.0** — `ltc`, `cue`, `chase`, `audio`, `sink`, `rtpmidi`,
`show`: el decodificador, las reglas de disparo, el chaser y las salidas. Las
mejoras a esos ficheros siguen abiertas y las puede usar cualquier cosa.

**El programa es GPL-3.0-or-later** — todo lo que hay bajo `apps/`, y `pablo`,
que lleva los dibujos. Pablo y los símbolos de transporte los dibujó Claude a
partir de un guion, ejemplos y correcciones de Leo Bolster.

### Sobre los parches

Por favor, **abre un issue en vez de un pull request.** No por antipatía: el
código fusionado pertenece a quien lo escribió, y un puñado de líneas aceptadas
pueden impedir para siempre que el autor licencie su propio trabajo de otra
manera más adelante. Describe el problema, o el arreglo, y se escribirá aquí
dándote crédito en el commit.
