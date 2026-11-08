# Kepler Industries

*Arbeitstitel.* Eine Wirtschaftssimulation in der Kartenansicht: Bahnen planen,
Fracht transportieren, Satellitennetze aufbauen, Basen gründen und alles am
Laufen halten. Geschrieben in Rust mit [Bevy](https://bevy.org).

Warum der Code so aufgebaut ist, steht in [`docs/ARCHITEKTUR.md`](docs/ARCHITEKTUR.md).

## Stand

Ein Planet mit Atmosphäre, eine Startbasis, zwei Satelliten mit Triebwerk.
„Relais-1" kreist stabil oberhalb der Atmosphäre. „Sonde-2" wurde zu tief
abgesetzt, wird bei jedem Durchgang durch die Luft abgebremst (orange) und
stürzt nach einigen Umläufen ab, außer man rettet sie mit einem Manöver.

Manöver plant man mit dem Manöver-Node wie in KSP, auch mehrere
hintereinander als Kette: Jeder Node liegt auf der Bahn nach den vorherigen
und wandert mit, wenn sich ein früherer ändert. Eine Kette ist sofort scharf:
Das Schiff brennt zu den geplanten Zeitpunkten selbst, auch unter Zeitraffer.
Ein Brennen beginnt nie vor jetzt und nie vor dem Ende des vorherigen, sonst
rückt der Node einen Umlauf weiter. Ein Node, für den es nach einer Änderung
keinen Platz mehr gibt (z. B. weil das Schiff vorher abstürzt), bleibt rot in
der Liste stehen, bis es wieder passt. Gelbe Ringe auf der Bahn zeigen Beginn
und Ende jedes Brennens. Während des Brennens zeigt die blaue Linie die momentane Bahn, die sich
sichtbar zur geplanten (violetten) hin verformt. Endet eine Bahn am Boden,
zeigt ein rotes Kreuz den Aufschlagpunkt.

![Manöver-Node](docs/screenshots/manoever-node.png)

*Rettung von Sonde-2: Node an der Apoapsis, 30 m/s prograd. Blau die aktuelle
Bahn, violett die Bahn danach.*

![Aufschlag](docs/screenshots/aufschlag.png)

*Relais-1 bremst ab: Die Bahn führt durch die Luft (orange) auf den Boden, das
Kreuz markiert den Aufschlagpunkt.*

![Manöverkette](docs/screenshots/manoeverkette.png)

*Hohmann-Übergang von 100 auf 500 km als Kette aus zwei Nodes: Der erste
(gewählt, mit Griffen) hebt die Apoapsis, der zweite liegt auf der violetten
Übergangsbahn an der Apoapsis und macht die Bahn rund.*

![Nächster Umlauf](docs/screenshots/naechster-umlauf.png)

*Node mit 250 m/s prograd, 20 s vor Relais-1 gesetzt. Das Brennen bräuchte
30 s Vorlauf, also liegt der Node einen Umlauf später; das Etikett „+1 Umlauf“
zeigt es an.*

## Starten

Voraussetzung: Rust ≥ 1.97.1 (`rustup update`). Unter Linux die
[Systempakete für Bevy](https://github.com/bevyengine/bevy/blob/main/docs/linux_dependencies.md)
(Audio- und Gamepad-Pakete werden nicht gebraucht).

```sh
cargo run -p game
```

Der erste Build dauert einige Minuten, danach geht es schnell.

### Steuerung

| Eingabe | Wirkung |
|---|---|
| Rechte Maustaste ziehen | Kamera drehen |
| Mausrad | Zoomen |
| Linksklick auf ein Schiff / `Tab` | Schiff wählen / nächstes Schiff |
| Linksklick auf die Bahn des gewählten Schiffs | Node setzen (auch auf die Bahn nach einem geplanten Manöver) |
| Linksklick auf einen Node / `↑` `↓` | Node wählen (nur der gewählte hat Griffe) |
| Griff ziehen | Δv in diese Richtung ändern (je weiter, desto schneller; `Umschalt`: fein; höchstens bis der Tank leer ist) |
| Node ziehen | Node entlang seiner Bahn verschieben; spätere Nodes wandern mit. Auf einer Bahn durch die Luft kommt er nach innen gezogen auf einen späteren Umlauf („+N Umläufe“) |
| `Entf` / `Rücktaste` | gewählten Node löschen |
| `H` | Zeitraffer-Stopp für das gewählte Schiff an/aus: Beginnt es zu brennen, hält der Zeitraffer genau dort an und schaltet auf 1× |
| `Esc` | Auswahl aufheben |
| `.` / `,` | Zeitraffer schneller / langsamer (1× bis 100 000×) |
| Leertaste | Pause |

Griffe: gelbgrün prograd/retrograd, violett normal/antinormal, türkis radial
nach außen/innen.

### Optionen

```sh
cargo run -p game -- --node 0,600,237,0,0 --node 0,2027,212,0,0 --camera -30,50,4200
```

- `--time SEKUNDEN`: vor dem Start bis zu dieser Spielzeit vorrechnen
- `--camera GIER,NEIGUNG,KM`: Blickwinkel in Grad, Abstand vom Planetenmittelpunkt in km
- `--select SCHIFF`: Schiff wählen (0 = Relais-1, 1 = Sonde-2)
- `--node SCHIFF,ZEIT,PROGRAD,NORMAL,RADIAL[,SCHARF]`: Node setzen (Zeit in s,
  Δv in m/s), mehrfach für eine Kette; scharf wie im Spiel, `SCHARF` = 0 lässt
  die Kette zum Testen nicht scharf
- `--screenshot DATEI.png`: Bild speichern und beenden (Zeit steht dabei still)

## Tests

```sh
cargo test -p sim
```

Die Simulation hat keine Abhängigkeit zu Bevy, die Tests kompilieren und laufen
in Sekunden.

## Aufbau

- `crates/sim`: Simulation (Bahnen, Atmosphäre, Manöver, Zeitleiste, Spielstand)
- `crates/game`: Darstellung und Eingabe mit Bevy
- `docs/ARCHITEKTUR.md`: Regeln, Physikmodell, Entscheidungen, Merkliste
