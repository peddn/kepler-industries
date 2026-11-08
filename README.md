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

Manöver plant man mit dem Manöver-Node wie in KSP. Das Schiff führt das
Brennmanöver zum geplanten Zeitpunkt selbst aus, auch unter Zeitraffer.
Während des Brennens zeigt die blaue Linie die momentane Bahn, die sich
sichtbar zur geplanten (violetten) hin verformt. Endet eine Bahn am Boden,
zeigt ein rotes Kreuz den Aufschlagpunkt.

![Manöver-Node](docs/screenshots/manoever-node.png)

*Rettung von Sonde-2: Node an der Apoapsis, 30 m/s prograd. Blau die aktuelle
Bahn, violett die Bahn danach.*

![Aufschlag](docs/screenshots/aufschlag.png)

*Relais-1 bremst ab: Die Bahn führt durch die Luft (orange) auf den Boden, das
Kreuz markiert den Aufschlagpunkt.*

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
| Linksklick auf die Bahn des gewählten Schiffs | Node setzen |
| Griff ziehen | Δv in diese Richtung ändern (je weiter, desto schneller; `Umschalt`: fein) |
| Node ziehen | Node entlang der Bahn verschieben |
| `Entf` / `Rücktaste` | Node löschen |
| `H` | Zeitraffer-Stopp für das gewählte Schiff an/aus: Beginnt es zu brennen, hält der Zeitraffer genau dort an und schaltet auf 1× |
| `Esc` | Auswahl aufheben |
| `.` / `,` | Zeitraffer schneller / langsamer (1× bis 100 000×) |
| Leertaste | Pause |

Griffe: gelbgrün prograd/retrograd, violett normal/antinormal, türkis radial
nach außen/innen.

### Optionen

```sh
cargo run -p game -- --time 1500 --node 1,1973.5,30,0,0 --camera -60,35,2600
```

- `--time SEKUNDEN`: vor dem Start bis zu dieser Spielzeit vorrechnen
- `--camera GIER,NEIGUNG,KM`: Blickwinkel in Grad, Abstand vom Planetenmittelpunkt in km
- `--select SCHIFF`: Schiff wählen (0 = Relais-1, 1 = Sonde-2)
- `--node SCHIFF,ZEIT,PROGRAD,NORMAL,RADIAL`: Node setzen (Zeit in s, Δv in m/s)
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
