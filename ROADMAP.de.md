# Fahrplan

[English version](ROADMAP.md)

Ausgangspunkt ist v0.2.3: ein GTK4/libadwaita-Launcher (Rust, relm4), der die
macOS-Spieldaten über NGDP installiert, sie mit dem Linux-Unity-Player
kombiniert, den Battle.net-Login abwickelt und das Spiel startet. Dieser
Fahrplan führt in vier Releases zu 1.0. Jedes Release ist für sich nutzbar.
Die Releases 0.3.0 bis 0.5.0 entstehen auf einem Branch und werden als ein
gemeinsamer Pull Request an Upstream gestellt, sobald 0.5.0 fertig ist; der
Shop folgt getrennt.

| Release | Thema | Umfang |
| --- | --- | --- |
| 0.3.0 | Übersetzbare Oberfläche (Locale-Katalog) | mittel |
| 0.4.0 | Startoptionen (GameMode, MangoHud) und Editor für Spieleinstellungen | mittel |
| 0.5.0 | Neuer Name, Fenstertitel, Logo und Icon | wenig Code, Grafikarbeit |
| 1.0.0 | Ingame-Shop über die originale macOS-Commerce-Bibliothek | groß, zuerst Analyse |

Der Katalog steht absichtlich am Anfang: 0.4.0 und 0.5.0 bringen viele neue
Texte, und die sollen von Beginn an über den Katalog laufen, statt
nachträglich umgestellt zu werden.

## Stand des Codes heute

| Bereich | Zustand | Ort |
| --- | --- | --- |
| Start | Startet `Bin/Hearthstone.x86_64` direkt oder über den Nix-Runtime-Runner; einzige Startoption ist „Discrete GPU“ | `src/install/launcher.rs` (`launch_game`, `graphics_env`) |
| Launcher-Einstellungen | `config.toml` mit Region, Locale, `use_discrete_gpu` | `src/config.rs`, `SettingsPanel` in `src/ui/components.rs` |
| Spielkonfiguration | `client.config` wird bei jeder Installation neu aus einer Vorlage erzeugt; die `options.txt` des Spiels wird nie angefasst | `src/install/compatibility.rs`, `assets/client.config.in` |
| Oberflächentexte | Rund 70 fest verdrahtete englische Texte; kein Katalog | `src/ui/app.rs`, `components.rs`, `status.rs`, `src/main.rs` |
| Name und Titel | Fenstertitel, Desktop-Eintrag und AppStream-Name lauten alle `hearthstone-linux-gui` | `src/ui/app.rs:116`, `data/` |
| Icon | Ein SVG, abgelegt beim AppImage-Packaging | `packaging/appimage/io.github.hearthstone_linux_gui.svg` |
| Shop | Ein Stub beantwortet 88 der 338 Einsprungpunkte, die das Spiel bindet, und liefert einen leeren Shop; der HTTP-Client-Stub tut nichts | `crates/stubs/blz-commerce-sdk-plugin`, `crates/stubs/commerce-http-client` |

## 0.3.0 Übersetzbare Oberfläche

Ziel: Kein sichtbarer Text ist fest verdrahtet; Englisch ist Basis und
Fallback; Deutsch ist vollständig.

- Locale-Katalog einführen, der in die Binärdatei einkompiliert wird
  (`locales/en.*`, `locales/de.*`). Eingebettete Kataloge ersparen die
  Installation von `.mo`-Dateien in fünf Paketformaten und halten den
  glibc-2.34-Build frei von einer `libintl`-Abhängigkeit.
- Alle Texte aus `src/ui/` und die CLI-Meldungen in `src/main.rs` auf
  Katalogschlüssel umstellen, auch die formatierten (`"Stop {action}"`,
  `"Failed: {message}"`).
- `launch_state()` in `src/ui/app.rs` leitet den Zustand derzeit aus dem
  Überschriftentext ab (`"Stopping game"`). Das wird vor dem Übersetzen durch
  ein echtes Zustandsfeld ersetzt, sonst bricht die Logik in jeder anderen
  Sprache.
- Die Oberflächensprache folgt der Systemsprache. Sie lässt sich über ein
  Zahnradmenü oben rechts umstellen, das jede Sprache mit ihrer Flagge
  zeigt (als mitgelieferte Grafik, nicht als Emoji, damit es nicht von
  installierten Schriften abhängt). Sie ist unabhängig von der bereits
  vorhandenen Spielsprache.
- Beim ersten Start werden Region und Spielsprache anhand der Systemsprache
  vorausgewählt, statt immer mit `us` / `enUS` zu beginnen.
- Der Fensterinhalt ist zentriert, damit Status, Einstellungen und Aktionen
  als eine geordnete Spalte gelesen werden.
- Desktop-Eintrag (`Name[de]`, `Comment[de]`) und AppStream-Metainfo
  (`xml:lang`) übersetzen.
- Ein Test lässt den Build scheitern, wenn in einem nicht-englischen Katalog
  ein Schlüssel fehlt oder ein Schlüssel ungenutzt ist.
- Chinesisch wird als dritter Katalog mitgepflegt; neben das vorhandene
  chinesische README kommen `README.de.md` und ein deutscher Screenshot.

Stand: auf dem Arbeitsbranch umgesetzt.

Fertig, wenn: Die App läuft mit `LANG=de_DE.UTF-8` vollständig auf Deutsch,
fällt bei einer nicht unterstützten Sprache auf Englisch zurück, und der
Katalogtest läuft in der CI.

## 0.4.0 Startoptionen und Spieleinstellungen

### GameMode und MangoHud

- Zwei Schalter im Einstellungsbereich, gespeichert in `config.toml`
  (`use_gamemode`, `use_mangohud`), standardmäßig aus.
- `launch_game` baut eine Wrapper-Kette vor die ausführbare Datei:
  `gamemoderun mangohud <Spiel>`. Die Wrapper müssen im `PATH` des Hosts
  gesucht werden (`host_env`), nicht in der AppImage-Umgebung.
- Ein Schalter ist deaktiviert und erklärt per Tooltip warum, wenn das
  Werkzeug nicht installiert ist. Die Erkennung läuft beim Start und bei
  „Refresh“.
- Unterschiede je Paketformat:
  - AppImage, DEB, RPM, Pacman: Die Werkzeuge des Hosts werden direkt genutzt.
  - Flatpak: Die Werkzeuge des Hosts sind aus der Sandbox nicht erreichbar;
    die Schalter sind deshalb deaktiviert und sagen das. Damit sie
    funktionieren, braucht es die Erweiterung
    `org.freedesktop.Platform.VulkanLayer.MangoHud` und Zugriff auf den
    D-Bus-Dienst von GameMode; das bleibt einem Folgeschritt vorbehalten.
  - Nix: Die Wrapper stehen vor dem Runtime-Runner; ob die FHS-Umgebung die
    Bibliotheken von MangoHud und GameMode zusätzlich braucht, muss unter
    NixOS getestet werden.
- Die Wrapper-Kette ist durch einen Unit-Test abgedeckt.

### Editor für Spieleinstellungen

Das Spiel liest zwei Dateien, die unterschiedlich behandelt werden müssen:

| Datei | Ort | Inhalt | Problem heute |
| --- | --- | --- | --- |
| `options.txt` | `~/Library/Preferences/Blizzard/Hearthstone/` (der macOS-Pfad, unter Linux unverändert benutzt) | Grafik- und Spieloptionen, `schlüssel=wert` | Gar nicht erreichbar |
| `client.config` | Spielverzeichnis | Verbindungs- und Spracheinstellungen | Wird bei jedem Install / Update überschrieben |

- Ein Dialog „Spieleinstellungen“, erreichbar über das Zahnradmenü, mit
  passenden Bedienelementen für die häufigen Fälle: Bildratenbegrenzung
  (`targetframerate`), VSync an oder aus, Grafikqualität.
- Ein Abschnitt „Erweitert“ mit einem einfachen Texteditor für `options.txt`
  und für zusätzliche `client.config`-Einträge, dazu „Im Editor öffnen“ und
  „Auf Standard zurücksetzen“.
- Unbekannte Schlüssel und ihre Reihenfolge bleiben beim Speichern erhalten.
  Das Spiel schreibt `options.txt` beim Beenden neu, deshalb ist das
  Bearbeiten gesperrt, solange das Spiel läuft.
- Eigene Änderungen an `client.config` werden als Überlagerung gespeichert und
  nach dem Erzeugen aus der Vorlage wieder angewendet, damit sie
  Install / Update überstehen.
- VSync wird abgeschaltet, indem `vsync=0` in die `options.txt` eingefügt
  wird. Der Schlüssel ist standardmäßig nicht vorhanden; der Editor fügt ihn
  bei Bedarf ein und entfernt ihn (oder setzt `vsync=1`), um VSync wieder
  einzuschalten.

Fertig, wenn: Bildrate und VSync lassen sich im Launcher ändern, die Änderung
ist im Spiel sichtbar (mit dem MangoHud-Overlay geprüft) und übersteht ein
Update.

Stand: auf dem Arbeitsbranch umgesetzt. Das Schreiben der Dateien ist
geprüft; die Wirkung im laufenden Spiel muss noch bestätigt werden.

## 0.5.0 Name, Logo und Icon

### Name und Fenstertitel

Der technische Name `hearthstone-linux-gui` bleibt als Name der Binärdatei,
Paketname und Anwendungs-ID (`io.github.hearthstone_linux_gui`) bestehen,
damit vorhandene Installationen, Datenverzeichnisse und der registrierte
Login-Handler weiter funktionieren. Es ändert sich nur der angezeigte Name.

- Vorschlag für den Anzeigenamen: **Hearthstone Linux Launcher**, mit dem
  Untertitel „Inoffizieller Community-Launcher“. Fenstertitel,
  Desktop-Eintrag, AppStream-Name und der Über-Text beziehen ihn aus dem
  Katalog.
- Der Untertitel ist nicht verhandelbar. `LEGAL.md` verpflichtet das Projekt
  darauf, nie wie ein offizielles Blizzard-Produkt auszusehen, und ein
  Launcher, der professioneller wirkt, braucht den Hinweis umso mehr.

### Logo und Icon

- Motiv: Tux spielt Karten an einem Tisch im Wirtshaus (Kamin, Fenster,
  Laternen). Tux ist die einzige Figur; glänzender,
  runder 3D-Maskottchenstil, passend zu den anderen Apps des Autors, leicht
  abgewandelt.
- Kein Blizzard-Material: kein Hearthstone-Logo oder -Wirbel, keine
  Kartenrahmen, Kartenbilder, Schriften oder Screenshots. Die Karten sind ein
  eigener Entwurf.
- Ergebnisse:
  - App-Icon als SVG in `data/icons/`, die einzige Quelle für AppImage, DEB,
    RPM, Pacman, Flatpak und Nix; die Paketskripte rendern daraus die
    PNG-Größen.
  - Logo im Hauptfenster direkt unter dem Titel.
  - Icon am Anfang der READMEs.
- Referenzmaterial aus der Entwurfsphase bleibt außerhalb des Repositorys.
- Ein symbolisches (einfarbiges) Icon ist nicht nötig: GNOME zeigt es für
  Anwendungen ohne Benachrichtigungen oder Statuseintrag nicht an.
- Die jetzige Zeichnung ist eine flache Vektorfassung. Eine gerenderte
  3D-Fassung im glänzenden Maskottchenstil kann sie später ersetzen, ohne
  dass sich am Code etwas ändert.

Fertig, wenn: Alle Paketformate zeigen das neue Icon und den neuen Namen im
Anwendungsmenü, und das Logo wird im hellen und dunklen Modus korrekt
dargestellt.

Stand: auf dem Arbeitsbranch umgesetzt; das paketierte Icon muss noch in
jedem Paketformat geprüft werden.

## 1.0.0 Ingame-Shop

### Idee

Installiert ist der macOS-Build, und dessen Shop steckt in einer nativen
macOS-Bibliothek, die der Launcher derzeit überspringt und durch einen leeren
Stub ersetzt. Statt den Shop nachzubauen, wird geklärt, wie er unter macOS
funktioniert, und dann eine **minimalistische Emulationsschicht** gebaut, die
genau diesen Bereich unter Linux laden und laufen lässt:

- Blizzards originale Commerce-Bibliothek wird unverändert geladen.
- Darum herum stellt eine kleine Schicht nur das Stück macOS-Umgebung
  bereit, das diese Bibliothek tatsächlich benutzt.
- Ihre Ein- und Ausgaben werden auf eine Kompatibilitätsschicht umgelegt,
  mit der der Linux-Unity-Player etwas anfangen kann; sie ersetzt den
  heutigen Stub.

Der Launcher enthält dann keine eigene Shop-Logik: Katalog, Preise,
Kaufabwicklung und Bezahlung erledigt Blizzards Code im Gespräch mit
Blizzards Servern. Die Originaldateien werden nie verändert.

### Was bisher bekannt ist

- Das Spiel bindet 338 Einsprungpunkte von `blz_commerce_sdk_plugin`
  (SWIG-Bindings in `Blizzard.Commerce.Bindings.dll`); der heutige Stub
  beantwortet 88 davon. Die Schnittstelle arbeitet mit Handles: Handles und
  Zeichenketten lassen sich durchreichen, ohne die Strukturen dahinter zu
  kennen.
- Ereignisse kommen über einen Listener-Callback zurück, angetrieben von
  `blz_commerce_update` einmal pro Bild.
- Die Kaufabwicklung ist kein eigenes Fenster. Die Bibliothek steuert einen
  unsichtbaren Browser (`BlizzardBrowser.app`, auf Chromium-Basis, wird mit
  dem Spiel bereits heruntergeladen) und übergibt dem Spiel Einzelbilder;
  das Spiel zeichnet sie und schickt Maus- und Tastatureingaben zurück
  (`blz_commerce_register_scene`, `blz_commerce_browser_send_event`).
- Die macOS-Bibliothek selbst ist ein Mach-O-Bundle. Linux kann sie nicht
  direkt laden; sie braucht einen Lader und die Systemfunktionen, die sie
  importiert.

### Phase 1: den macOS-Shop verstehen

Vorher wird nichts gebaut, denn die Antworten entscheiden, wie klein die
Emulationsschicht sein kann.

1. Die originale Commerce-Bibliothek bei der Installation behalten (in einem
   eigenen Verzeichnis, verifiziert wie alle anderen NGDP-Inhalte), statt sie
   zu überspringen.
2. Ihre Mach-O-Ladebefehle lesen: welche Systembibliotheken und Frameworks
   sie einbindet, und die vollständige Liste der importierten Symbole. Diese
   Liste **ist** der Umfang der zu simulierenden macOS-Umgebung.
3. Die Importe sortieren: reine C- und C++-Laufzeit (lässt sich fast eins zu
   eins auf Linux abbilden), CoreFoundation (das Projekt liefert bereits
   einen Teil-Stub), Netzwerk und TLS, und alles mit Objective-C oder Cocoa.
4. Den Ablauf auf der Managed-Seite nachvollziehen: Initialisierung,
   HTTP-Registrierung, Laden der Produkte, personalisierter Shop,
   Kaufabwicklung, Kaufereignisse.
5. Klären, wie die Bibliothek mit `BlizzardBrowser` spricht: wie er
   gestartet wird, über welchen Kanal Bilder und Eingaben laufen.

Ergebnis: eine schriftliche Import- und Ablaufkarte sowie ein Ja oder Nein
für jeden der beiden folgenden Teile.

### Phase 2: Emulationsschicht für die Commerce-Bibliothek

```
Hearthstone (Linux-Unity-Player)
  │  P/Invoke, 338 Einsprungpunkte
  ▼
libblz_commerce_sdk_plugin.so      ← eigener Code: Kompatibilitätsschicht
  │  lädt und reloziert das Mach-O-Abbild, bindet dessen Importe
  ▼
originale macOS-Commerce-Bibliothek ← von Blizzard, unverändert
  │  importierte Systemaufrufe
  ▼
macOS-Umgebungs-Shim               ← eigener Code: nur was Phase 1 als benutzt findet
  │
  ▼
Linux-libc, TLS, Sockets, Dateien
```

- Ein Mach-O-Lader in der Kompatibilitätsbibliothek: Segmente einblenden,
  Relokationen anwenden und die Importtabelle an den Shim binden. Beide
  Seiten sind x86_64 und nutzen dieselbe C-Aufrufkonvention; eine
  CPU-Emulation ist nicht beteiligt.
- Der Shim setzt die importierten Funktionen auf ihre Linux-Entsprechungen
  um. Pfade, die die Bibliothek unter `~/Library` erwartet, werden auf die
  Verzeichnisse des Spiels abgebildet.
- Die exportierten Einsprungpunkte werden aus dem Bindings-Assembly erzeugt,
  damit ein Spielupdate mit neuen Einsprungpunkten ein Neubau ist und keine
  Handarbeit.
- Fehlt die Originalbibliothek oder lässt sie sich nicht laden, verhält sich
  die Schicht wie der heutige Stub: Der Shop bleibt leer, das Spiel läuft
  normal.

Meilenstein: Der Shop zeigt echte Produkte.

### Phase 3: Browser für die Kaufabwicklung

`BlizzardBrowser.app` ist eine vollständige Chromium-Anwendung für macOS. Sie
hängt an Cocoa und am Grafiksystem von macOS, und das kann eine
minimalistische Schicht nicht simulieren. Der realistische Weg ist, das
Protokoll zu behalten und den Prozess zu ersetzen: ein kleiner unsichtbarer
Linux-Browser, der den in Phase 1 gefundenen Kanal spricht und dieselben
Kaufseiten von Blizzard lädt. Ob das so tragbar ist oder die Kaufabwicklung
stattdessen im Systembrowser geöffnet wird, entscheidet sich mit den
Ergebnissen aus Phase 1.

Meilenstein: Ein vom Kontoinhaber getätigter Kauf kommt im Spiel an.

### Risiken

- Die Importliste kann groß ausfallen oder die Objective-C-Laufzeit
  enthalten. Jede solche Abhängigkeit wird entweder nachgebildet, oder sie
  beendet diesen Ansatz.
- Unterschiede der C++-Laufzeit (`libc++` unter macOS gegenüber `libstdc++`
  unter Linux) müssen abgefangen werden, indem für das geladene Abbild eine
  passende Laufzeit mitgeliefert wird.
- Ausweichweg, falls Phase 1 Nein sagt: den **Windows**-Build derselben
  Bibliothek unverändert in einem Hilfsprozess unter Wine ausführen und mit
  dem Spiel verbinden. Das Prinzip bleibt gleich (Originalcode, unangetastet),
  kostet aber eine optionale Wine-Abhängigkeit.
- Ein nativer Nachbau des Shop-Protokolls bleibt ausgeschlossen.

### Regeln für dieses Release

- Sitzungstoken und alles, was die Schicht passiert, bleiben auf dem Rechner
  und werden nie in Logs geschrieben.
- Der Launcher löst nie einen Kauf aus, verändert oder wiederholt keinen. Er
  reicht Aufrufe durch, sonst nichts.
- Entwickelt wird nur mit dem Durchblättern des Katalogs. Echte Käufe als
  Abschlusstest macht der Kontoinhaber selbst, bewusst und mit kleinem
  Betrag.
- `LEGAL.md` wird für dieses Release neu geschrieben. Heute steht dort, das
  Projekt berühre den Handel nicht, und im Abschnitt zu den Stubs, sie gäben
  keinen Zugang über den offiziellen Client hinaus. Beide Aussagen müssen den
  neuen Aufbau zutreffend beschreiben: Originalbibliothek, unverändert, durch
  die eigene Installation des Nutzers von Blizzard geladen, nicht
  weiterverteilt.
- „Lizenzkonformer“ ist eine technische Aussage darüber, Blizzards Code nicht
  nachzubauen und nicht zu verändern. Eine rechtliche Freigabe ist es nicht:
  Den Client unter Linux zu betreiben liegt außerhalb dessen, was Blizzard
  unterstützt, und das bleibt auch mit funktionierendem Shop so. Das README
  sagt das klar.

Fertig, wenn: Der Shop listet Produkte, ein vom Kontoinhaber getätigter Kauf
kommt im Spiel an, das Spiel läuft weiter, wenn die Schicht sich nicht laden
lässt, und ein Spielupdate erfordert keine Handarbeit.

## Entscheidungen

1. **Repository.** Entschieden: Die Arbeit geht an das Upstream-Projekt. Die
   Releases 0.3.0 bis 0.5.0 kommen in einen Pull Request, sobald 0.5.0 fertig
   ist. Projekt-URL, Copyright-Zeile und Anwendungs-ID bleiben, wie sie sind.
2. **Katalogformat.** Entschieden: in die Binärdatei eingebettete Kataloge,
   keine neue Abhängigkeit.
3. **Shop.** Entschieden: die macOS-Umgebung für die originale
   macOS-Bibliothek emulieren, der Weg über Wine nur als Ausweichlösung.
4. **Anzeigename.** Entschieden: „Hearthstone Linux Launcher“ mit dem
   Untertitel „Inoffizieller Community-Launcher“.
