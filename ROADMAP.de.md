# Roadmap

[English version](ROADMAP.md)

Ausgangspunkt ist v0.2.3: ein GTK4/libadwaita-Launcher (Rust, relm4), der die
macOS-Spieldaten über NGDP installiert, mit dem Linux-Unity-Player
zusammenführt, die Battle.net-Anmeldung übernimmt und das Spiel startet. Jeder
Schritt ist für sich nutzbar und kommt als kleiner, fokussierter Pull Request.
Die Details zu den einzelnen Schritten stehen in den Pull Requests und in
`CHANGELOG.md`.

| Schritt | Thema | Stand |
| --- | --- | --- |
| 0.3.0 | Übersetzbare Oberfläche (Sprachkataloge in `locales/`, Deutsch und Chinesisch) | gemergt (#29) |
| 0.4.0 | Startoptionen (GameMode, MangoHud) und Editor für die Spieleinstellungen | gemergt (#30, #31) |
| 0.5.0 | Anzeigename, Logo und Icon (kompaktes Layout und Update-Prüfung folgen) | in Prüfung |
| 1.0.0 | Shop im Spiel | offene Frage, siehe unten |

## 0.5.0 Name, Logo und Icon

- Der angezeigte Name ist **Hearthstone Linux Launcher**. Binärname,
  Paketname und Anwendungs-ID (`io.github.hearthstone_linux_gui`) bleiben
  unverändert, damit bestehende Installationen, Datenverzeichnisse und der
  registrierte Login-Handler weiter funktionieren.
- Überall, wo der Name erscheint, steht auch, dass es sich um einen
  inoffiziellen Community-Launcher handelt: im Fenstertitel als Untertitel,
  im Desktop-Eintrag und in der AppStream-Kurzbeschreibung als Beginn des
  Textes. `LEGAL.md` verpflichtet das Projekt, nie wie ein offizielles
  Blizzard-Produkt zu wirken, und ein Launcher, der professioneller aussieht,
  braucht den Hinweis umso mehr.
- Das Icon ist eine eigene Zeichnung (Tux spielt Karten an einem Tisch in
  einer Taverne) ohne jedes Blizzard-Material: kein Logo, keine Kartenrahmen,
  keine Kartenmotive, keine Schriften. Es liegt in `data/icons/`, der
  gemeinsamen Quelle für alle Paketformate, und erscheint als Logo unter dem
  Fenstertitel.
- Offen: das Icon in den Anwendungsmenüs von Flatpak, AppImage und Nix prüfen.

## 1.0.0 Shop im Spiel

Das installierte Spiel ist der macOS-Build, dessen Shop in einer nativen
macOS-Bibliothek steckt. Der Launcher überspringt sie und ersetzt sie durch
einen leeren Stub (`crates/stubs/blz-commerce-sdk-plugin`,
`crates/stubs/commerce-http-client`).

Ob und wie sich der Shop unterstützen lässt, ist eine offene Frage, die
zuerst von einem rechtlich gangbaren Weg abhängt. Sie wird in #26 besprochen;
bis dahin ist hier nichts geplant oder zugesagt.
