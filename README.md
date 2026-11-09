# Kepler Industries

*Arbeitstitel.* Eine Wirtschaftssimulation in der Kartenansicht: Bahnen planen,
Fracht transportieren, Satellitennetze aufbauen, Basen gründen und alles am
Laufen halten. Geschrieben in Rust mit [Bevy](https://bevy.org).

Warum der Code so aufgebaut ist, steht in [`docs/ARCHITEKTUR.md`](docs/ARCHITEKTUR.md).

## Stand

Ein Planet mit Atmosphäre, eine Startbasis, zwei Satelliten mit Triebwerk und
eine Fähre am Boden. „Relais-1" kreist stabil oberhalb der Atmosphäre.
„Sonde-2" wurde zu tief abgesetzt, wird bei jedem Durchgang durch die Luft
abgebremst (orange) und stürzt nach einigen Umläufen ab, außer man rettet sie
mit einem Manöver. „Fähre-3" steht an der Startbasis und kann starten.

Gestartet wird mit einem Startprofil aus drei Werten: Kurs, Wendebeginn und
Ziel-Apoapsis. Eingestellt wird es in der Seitenleiste rechts, die erscheint,
sobald ein gelandetes Schiff gewählt ist. Die Vorschau zeigt den ganzen
Aufstieg (gelb) und die Bahn danach. Den Node zum Zirkularisieren an der
Apoapsis kann man schon vor dem Abheben setzen.

Gelandet wird über einen Node, der die Bahn in die Luft führt. Die Vorschau
rechnet bis zum Boden, mit Fallschirm (unter 3 km). Ein Kreuz zeigt, wo das
Schiff aufsetzt: grün im Zielgebiet (grüne Kreise), gelb außerhalb, rot bei
einem Absturz. Die letzte Bremsung ist abstrahiert: Die Geschwindigkeit am
Boden muss als Δv abgebremst werden, mit genug Treibstoff und Schub größer als
das Gewicht. Wer daneben landet, kommt mit einem Hop ins Ziel: einem Start mit
niedriger Ziel-Apoapsis, zum Beispiel von der Startbasis zum Übungsfeld 60 km
östlich (Kurs 90°, Ziel-Apoapsis etwa 27 km). Relais-1 und Sonde-2 sind
Satelliten ohne Fallschirm und mit weniger Schub als Gewicht: Sie stürzen beim
Landen immer ab. Die Anzeige nennt dann Ort (auch das Zielgebiet) und Grund.

Unten zeigt ein Balken den Tank des gewählten Schiffs als Δv: Die volle Breite
ist der volle Tank, darin die geplanten Brennvorgänge der Kette (Start gelb,
Nodes violett), dahinter der Rest (grün). Reicht der Treibstoff nicht, steht
rot dahinter, wie viel fehlt.

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

![Landung](docs/screenshots/landung.png)

*Hop der Fähre von der Startbasis (links) zum Übungsfeld: Aufstieg gelb,
Flug durch die Luft orange, das grüne Kreuz liegt im Zielgebiet. Die Bahn ist
ohne Drehung des Planeten gezeichnet, das Kreuz zeigt den genauen Punkt auf
der Oberfläche.*

![Startprofil](docs/screenshots/startprofil.png)

*Fähre-3 vor dem Start: Aufstieg nach Osten (gelb), Freiflug durch die obere
Luft (orange), dazu ein Node an der Apoapsis, der die Bahn auf 99 km rund
macht. Rechts die Karte „Start“.*

![Manöver-Node](docs/screenshots/manoever-node.png)

*Rettung von Sonde-2: Node an der Apoapsis, 30 m/s prograd. Blau die aktuelle
Bahn, violett die Bahn danach.*

![Aufschlag](docs/screenshots/aufschlag.png)

*Relais-1 bremst ab: Die Bahn führt durch die Luft (orange) auf den Boden. Ein
Satellit hat zu wenig Schub zum Abbremsen, das rote Kreuz markiert den
Absturz.*

![Manöverkette](docs/screenshots/manoeverkette.png)

*Hohmann-Übergang von 100 auf 500 km als Kette aus zwei Nodes: Der erste
(gewählt, mit Griffen) hebt die Apoapsis, der zweite liegt auf der violetten
Übergangsbahn an der Apoapsis und macht die Bahn rund.*

![Nächster Umlauf](docs/screenshots/naechster-umlauf.png)

*Node mit 250 m/s prograd, 20 s vor Relais-1 gesetzt. Das Brennen bräuchte
30 s Vorlauf, also liegt der Node einen Umlauf später; das Etikett „+1 Umlauf“
zeigt es an.*

![Δv-Balken](docs/screenshots/dv-balken.png)

*Relais-1 hat noch 669 m/s an Bord (von 1216 m/s mit vollem Tank). Die Kette
will zweimal 400 m/s: Der zweite Node bekommt nur 269 m/s, 131 m/s fehlen.*

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
| Seitenleiste „Start“ (gelandetes Schiff gewählt) | Start planen / abbrechen; −/+ ändern Startzeit, Kurs, Wendebeginn, Ziel-Apoapsis (`Umschalt`: feine Schritte) |
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
- `--select SCHIFF`: Schiff wählen (0 = Relais-1, 1 = Sonde-2, 2 = Fähre-3)
- `--launch SCHIFF,ZEIT,KURS,WENDE_KM,AP_KM`: Start planen (Zeitpunkt des
  Abhebens in s, Kurs in Grad), z. B. `--launch 2,120,90,2,100`
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

- `crates/sim`: Simulation (Bahnen, Atmosphäre, Manöver, Start, Zeitleiste, Spielstand)
- `crates/game`: Darstellung und Eingabe mit Bevy (Anzeige links, Seitenleiste rechts)
- `docs/ARCHITEKTUR.md`: Regeln, Physikmodell, Entscheidungen, Merkliste
