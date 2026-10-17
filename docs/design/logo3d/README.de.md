# Animiertes Logo

[English version](README.md)

Quelle des animierten Launcher-Logos (`data/logo/logo-loop.webm`) und der
README-Animation (`docs/images/logo-animation.webp`): Tux steht hinter dem
Ausgabetisch einer Taverne und spielt Karten, wie im gezeichneten Logo
(`data/icons/logo.png`), nur in 3D modelliert. Es ist ein eigenes Werk ohne
Blizzard-Material: kein Logo, keine Kartenrahmen, keine Kartenmotive, keine
Schriften.

| Datei | Zweck |
| --- | --- |
| `build_logo.py` | Baut die Szene `HS_Logo` (Tux, Karten, Tisch, Taverne im runden Rahmen, Licht, Kamera). Der Aufbau folgt den SVG-Koordinaten des gezeichneten Logos. |
| `animate_logo.py` | Macht daraus eine zehnsekündige Schleife (240 Bilder bei 24 fps): Das Feuer flackert, Laternen und Sterne leuchten unruhig, Tux atmet, blinzelt, schaut sich seine Karten an und hebt die mittlere an. Stellt die Ausgabe auf 640-px-PNG mit Alphakanal. |
| `check_overlaps.py` | Listet Objekte, deren Oberflächen sich durchdringen. Jedes Objekt darf nur auf, vor oder hinter einem anderen liegen; Teile eines Gegenstands sind zusammengefasst. |
| `encode.sh` | Kodiert die Einzelbilder als WebM (368 px, VP9 mit Alphakanal) und WebP (320 px). |

## Neu erzeugen

Erstellt mit Blender 5.4 (EEVEE); die Skripte ändern nur die Szene `HS_Logo`.

1. In Blender `build_logo.py` und danach `animate_logo.py` ausführen
   (Arbeitsbereich Scripting oder `exec(open(pfad).read())` in der
   Python-Konsole).
2. Als Ausgabepfad einen Ordner mit `frame_####` am Ende setzen
   (Ausgabe-Eigenschaften) und die Animation rendern (Rendern > Animation
   rendern).
3. Die Einzelbilder kodieren: `docs/design/logo3d/encode.sh <Bildordner>`.

Oder im Hintergrund (EEVEE braucht zum Rendern eine Anzeige, zum Beispiel
über `xvfb-run`):

```sh
xvfb-run -a blender -b --python docs/design/logo3d/build_logo.py \
  --python docs/design/logo3d/animate_logo.py \
  -S HS_Logo -o /tmp/logo-frames/frame_#### -a
docs/design/logo3d/encode.sh /tmp/logo-frames
```

Um Durchdringungen zu prüfen, `check_overlaps.py` in derselben Sitzung nach
den beiden anderen Skripten ausführen; vorher `FRAMES` (ein Tupel von
Bildnummern) setzen, um mehr als Bild 1 zu prüfen. Berührende Flächen
erscheinen als wenige Dreieckspaare und sind in Ordnung.
