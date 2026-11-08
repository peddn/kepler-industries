# Architektur und Regeln

Dieses Dokument hält fest, *warum* der Code so aufgebaut ist. Die meisten Regeln
stammen aus einer Analyse der technischen Probleme von Kerbal Space Program 1
und 2. Wer etwas ändert, das einer Regel widerspricht, ändert bitte zuerst
dieses Dokument (mit Begründung).

## Aufbau

| Crate | Aufgabe | Abhängigkeiten |
|---|---|---|
| `crates/sim` | Die gesamte Simulation: Himmelskörper, Bahnen, Atmosphäre, Schiffe, Zeit. Der Zustand (`World`) ist zugleich der Spielstand. | nur `glam` (f64-Mathe) und `serde` |
| `crates/game` | Darstellung und Eingabe mit Bevy. Liest die Simulation und treibt sie voran. | `sim`, `bevy` |

`sim` kennt Bevy nicht. Dadurch kompiliert es in Sekunden, ist vollständig
ohne Fenster testbar (`cargo test -p sim`), und es gibt keinen Spielzustand,
der nur in Bevy-Komponenten steckt.

Einheiten: SI (m, s, kg), alles in `f64`. Zeit = Sekunden seit Spielbeginn.

## Regeln

### 1. Rechengenauigkeit
*KSP:* Unity rechnet in 32 Bit, KSP bräuchte 64 Bit. Folge: zitternde Schiffe,
der „Deep Space Kraken", aufwendige Gegenmaßnahmen (Floating Origin, Krakensbane).

**Regel:** Die Simulation rechnet nur in `f64`. Bevy bekommt jedes Bild nur
Positionen *relativ zum Kamerafokus* als `f32` (`camera::to_view`). Der Fokus
liegt in Bevy im Ursprung, die Kamera ist nie weiter entfernt als der
Zoomabstand. Es gibt keine `f32`-Weltkoordinaten.

### 2. Ein einziges Bezugssystem
*KSP:* Schaltet unterhalb von ~100 km auf ein mitrotierendes System um.
Folge: Geschwindigkeitsangaben weichen um die Rotationsgeschwindigkeit ab,
Positionen sind inkonsistent, Spieler vermuten Bahndrift in genau diesen Höhen.

**Regel:** Jede Bahn ist im nicht rotierenden System ihres Körpers
beschrieben (Ursprung im Mittelpunkt, +Z = Nordpol). Die Rotation steckt nur
in den Umrechnungen Oberfläche ↔ Raum in `body.rs`.

### 3. Keine Bahndrift
*KSP:* Bahnen werden bei Zeitraffer-, Fokus- und SOI-Wechseln neu berechnet und
gerundet; Satellitennetze laufen auseinander. KSP 2 hatte einen „Orbital
Decay"-Bug, der per Patch nur „stark reduziert" wurde.

**Regel:** Ein Keplerabschnitt speichert seinen Zustand zu Beginn (Epoche) und
verändert ihn nie. Jede Position wird direkt daraus berechnet
(`kepler::propagate`), bei Ellipsen nach Abzug ganzer Umläufe. Neue Bahnen
entstehen nur durch ausdrückliche Ereignisse.
*Test:* `no_drift_after_thousand_orbits`, `stable_orbit_never_changes`.

### 4. Kein „aktives Schiff", keine Physik-Blase
*KSP:* Nur Schiffe im Umkreis von ~2,5 km werden voll simuliert. Andere fliegen
ohne Luftwiderstand oder werden in der Atmosphäre gelöscht. Für ein Spiel mit
automatischen Routen wäre das fatal.

**Regel:** Alle Schiffe werden gleich behandelt, egal ob sie im Bild sind.
Flugabschnitte mit Luft oder Schub werden einmal vorausberechnet und danach
nur abgespielt (`Segment::Integrated`).

### 5. Zeitraffer gilt immer
*KSP:* Kein Schub unter Zeitraffer, Ionentriebwerke brennen stundenlang in Echtzeit.

**Regel:** Weil berechnete Abschnitte nur abgespielt werden, funktioniert der
Zeitraffer in jeder Lage, auch in der Luft und beim Brennen.

### 6. Ergebnis unabhängig von der Schrittgröße
*KSP:* SOI-Wechsel im Hintergrund weichen bei hohem Zeitraffer von der Vorhersage ab.

**Regel:** Ereignisse (Eintritt in die Luft, Austritt, Bodenkontakt, Beginn und
Ende eines Brennmanövers) werden exakt bestimmt (Keplerbahn: analytisch,
berechneter Abschnitt: im Schritt interpoliert bzw. genau bis zum Zeitpunkt
gerechnet). Ein Abschnitt hängt nur von seinem Anfangszustand und dem
geplanten Manöver ab. Der Integrator hat eine feste Schrittweite, unabhängig
von Bildrate und Zeitraffer.
*Test:* `result_is_independent_of_step_size`, `node_result_is_independent_of_step_size`
(1 Schritt == 10 000 Schritte, bitgenau).

### 7. Keine Bauteil-Physik
*KSP:* Wackelnde Raketen, Lag ab einigen hundert Teilen, Phantomkräfte. In KSP 2
erzeugte ein Fix gegen das Wackeln einen wochenlang unbemerkten Treibstoff-Bug.

**Regel:** Ein Schiff ist ein Massepunkt mit Werten. Keine Physik-Engine.

### 8. Am Boden ist ein Zustand
*KSP:* Gelandete Schiffe hüpfen oder explodieren beim Laden.

**Regel:** „Am Boden" ist ein Abschnitt mit Körper, Breite und Länge
(`Segment::Surface`). Er dreht sich mit dem Körper und kann nicht hüpfen.

### 9. Viele Schiffe sind billig
*KSP:* Auch Schiffe „auf Schienen" und Trümmer kosten Rechenzeit.

**Regel:** Jedes Schiff kennt das Ende seines aktuellen Abschnitts. Pro Bild ist
das ein Zahlenvergleich. Gerechnet wird nur, wenn ein Abschnitt endet. Wracks
am Boden kosten nichts. Eine globale Ereignis-Warteschlange kommt erst, wenn
Schiffe miteinander wechselwirken (Rendezvous).

### 10. Kleine, spielbare Schritte
*KSP 2:* Studio 2024 geschlossen, versprochene Inhalte nie geliefert.

**Regel:** Jeder PR ist klein und spielbar. Die Simulation wird durch Tests
abgesichert, die ohne Bevy laufen.

### 11. Spielstände bitgenau
Der Test `save_and_load_round_trip` hat gezeigt: `serde_json` liest Zahlen
ohne die Option `float_roundtrip` nicht bitgenau zurück. Jedes künftige
Speicherformat muss Zahlen exakt erhalten. Der Spielstand trägt eine
Versionsnummer (`SAVE_VERSION`).

### 12. Die Vorschau ist die Ausführung
Ein Manöver-Node ist der Plan selbst: `add_node`/`set_node` berechnen die
Zeitleiste neu, und diese Zeitleiste wird später genau so abgespielt. Es gibt kein eigenes
Vorschausystem, das von der Ausführung abweichen könnte.
*Test:* `impact_prediction_matches_wreck`, `clearing_node_restores_trajectory`.

### 13. Bedienelemente auf eigener Ebene
Node und Griffe zeichnet eine zweite Kamera mit eigenem Tiefenpuffer
(`OVERLAY_LAYER`). Sie liegen damit immer über der Welt und verschwinden nicht
im Planeten. Der Versuch mit `depth_bias = -1` zeichnete Linien nur
bruchstückhaft.

## Physikmodell

**Ein Kraftmodell überall:** Schwerkraft + Luftwiderstand + Schub.
Ein Körper ohne Luft ist der Spezialfall Bodendichte 0, Freiflug der
Spezialfall „kein Schub".

**Atmosphäre:** verschobene Exponentialfunktion

    ρ(h) = ρ₀ · (e^(−h/H) − e^(−h_max/H)) / (1 − e^(−h_max/H))   für h < h_max, sonst 0

Am Boden ρ₀, bei `h_max` exakt 0, dazwischen stetig. Oberhalb von `h_max` ist
die Keplerbahn deshalb die *exakte* Lösung desselben Modells, keine Näherung.
Eine reine Exponentialfunktion würde nie null; dann müsste jede Bahn ewig
schrittweise gerechnet werden (Drift, Rechenzeit).

**Luftwiderstand:** `a = −½ · ρ · (c_w·A / m) · |v_rel| · v_rel`, mit `v_rel`
relativ zur mitrotierenden Luft.

**Brennmanöver:** konstanter Schub, konstanter Massenstrom. Die Masse wird
analytisch aus der Zeit berechnet (nicht integriert), dadurch gilt die
Raketengleichung exakt. Die Dauer folgt aus dem Δv:
`m_end = m₀ · e^(−Δv/v_e)`, `Dauer = (m₀ − m_end) / ṁ`. Das Manöver liegt
zeitlich mittig um den Node, die Schubrichtung bleibt im Raum fest (wie in KSP
beim Zielen auf den Node). Reicht der Treibstoff nicht, endet es mit leerem Tank.

**Manöverkette:** Ein Schiff hat eine Kette von Nodes. Jeder liegt auf der
Bahn nach den vorherigen Brennvorgängen; die Zeitleiste rechnet sie der Reihe
nach aus (`Trajectory::rebuild`). Gespeichert wird pro Node, wohin er gesetzt
wurde (Zeit, Δv); Lage, Richtung und Brennzeiten folgen daraus (`Plan`).
Ändert sich ein früherer Node, behalten spätere Zeit und Δv und wandern mit
der Bahn.

**Brennen ganz in der Zukunft und nacheinander:** Ein Brennen beginnt nicht
vor jetzt und echt nach dem Ende des vorherigen. Sonst rückt der Node genau
einen Umlauf weiter (gleiche Stelle seiner Bahn, alle Zeiten + Umlaufzeit),
so oft wie nötig. Das geht nur auf einer geschlossenen Bahn, die bis zum Ende
des verschobenen Brennens weder Luft noch Boden berührt. Rückt ein Node nur
wegen eines früheren weiter, wird das jedes Mal neu berechnet, nicht
gespeichert; nimmt man das frühere Manöver zurück, kehrt er zurück.

**Ungültige Nodes:** Gibt es für einen Node auf seiner Bahn keinen Platz
(Absturz vorher `OnGround`, kein nächster Umlauf `TooLate`/`Overlaps`, hinter
der Vorausberechnung), ist er ungültig. Beim Setzen oder Ändern wird so ein
Node abgelehnt. Wird er es erst durch die Änderung eines früheren, bleibt er
erhalten, wird nicht ausgeführt und erscheint nur in der Liste (rot). Alle
späteren sind dann auch ungültig (`AfterInvalid`). Passt es wieder, ist er
wieder gültig.

**Nicht scharfe Kette:** Erreicht sie ihren ersten Brennbeginn, rückt sie als
Ganzes einen Umlauf weiter (nach derselben Regel) oder verfällt.

**Start vom Boden (Startprofil):** drei Werte: Kurs (90° = Osten), Wendebeginn
(Höhe), Ziel-Apoapsis. Senkrecht bis zum Wendebeginn, dann zunehmend geneigt
(Neigung aus der Senkrechten = 90° · √Anteil, anfangs also schneller),
waagerecht ab der halben Ziel-Apoapsis (`FLAT_FRACTION`). Die Schubrichtung
bleibt in einer festen Ebene durch Startplatz und Kurs (Großkreis); die
Bahnneigung ergibt sich daraus und aus der Drehung des Planeten. Der Aufstieg
ist ein normales `Burn` mit Aufstiegssteuerung statt fester Richtung, deshalb
gelten Vorschau = Ausführung, Brennpunkte und Nodes danach ohne Sonderweg.
Brennschluss, wenn die Apoapsis die Zielhöhe erreicht: beim Planen mit
denselben Schritten wie die Zeitleiste gerechnet und im Schritt auf Bruchteile
eines Meters genau gesucht, dann als Ende des Brennens gespeichert. Danach
Freiflug; die obere Luft senkt die Apoapsis noch etwas, die Vorschau zeigt den
echten Wert. Mit leerem Tank vorher: Brennschluss dort, die Vorschau zeigt den
Absturz. Kein Start bei Schub ≤ Gewicht. Der Start steht vor allen Nodes und
gehört zur Kette; Nodes danach liegen auf der Bahn nach dem Aufstieg und
lassen sich schon vor dem Abheben setzen. Wird die Startzeit verschoben,
wandert die ganze Kette mit. Ein nicht scharfer Start wartet eine
Planetendrehung (dann liegt der Startplatz wieder gleich).

**Zeitleiste eines Schiffs:** Folge von Abschnitten
- `Kepler`: Freiflug ohne Luft, analytisch (universelle Variablen: eine Formel
  für Ellipse, Parabel, Hyperbel).
- `Integrated`: vorausberechnet (RK4, 0,1 s, Stützpunkt jede Sekunde,
  Hermite-Interpolation dazwischen, max. 30 min pro Stück); mit oder ohne Schub.
- `Surface`: am Boden (gelandet oder Wrack); mit geplantem Start endet er beim
  Abheben.

Vorausberechnet werden 8 Abschnitte (`LOOKAHEAD`) nach dem Ende des letzten
geplanten Brennens (ohne Manöver: ab jetzt), oder bis die Zeitleiste offen
endet (stabile Bahn, am Boden). Das reicht, um nach dem letzten Manöver die
neue Bahn oder den Aufschlag zu sehen. Auf einer Bahn, die bei jedem Umlauf
durch die Luft führt, sind 8 Abschnitte vier Umläufe; weiter voraus lässt
sich kein Node setzen.
Übergänge: Kepler → (Eintritt) → Integrated → (Austritt) → Kepler;
Freiflug → (Brennbeginn) → Integrated mit Schub → (Brennende) → Freiflug;
Surface → (Abheben) → Integrated mit Aufstiegssteuerung → (Brennschluss) → Freiflug;
→ (Bodenkontakt) → Surface/Wrack.

**Aufschlagkreuz:** Es markiert die Stelle *auf dem Boden* und dreht sich mit
dem Planeten. Die Bahnlinie ist im nicht rotierenden System gezeichnet. Bis
zum Aufschlag dreht sich der Planet noch, deshalb liegt das Kreuz vorher nicht
am Ende der Linie; beide treffen sich genau beim Aufschlag. Für den Vergleich
mit Zielgebieten (die sich ebenfalls mitdrehen) ist das die richtige Größe.

## Getroffene Spielentscheidungen

- 3D, kleine Planeten (Kerbin-Größe), **keine Raketenstufen**.
- **Aufschlagkreuz und Bahnlinie:** Das Kreuz zeigt den genauen Punkt auf dem
  Boden (dreht mit dem Planeten), die Bahn wird ohne Planetendrehung
  gezeichnet. Dass das Ende der Linie vor dem Aufschlag nicht am Kreuz liegt,
  ist gewollt (siehe „Aufschlagkreuz“ unter Physikmodell).
- **Manöverketten:** mehrere Nodes pro Schiff (siehe „Manöverkette“ unter
  Physikmodell). Ein Klick auf einen beliebigen gezeichneten Teil der Zukunft
  fügt einen Node ein, auch auf die Bahn nach einem geplanten Manöver. Ein
  Klick auf einen Node (oder `↑`/`↓`) wählt ihn; nur der gewählte hat Griffe,
  `Entf` löscht ihn. Verschoben wird ein Node entlang seiner Bahn (der Bahn
  nach den früheren Manövern). Auf einer zerfallenden Bahn liegen spätere
  Umläufe weiter innen; zieht man dorthin, kommt der Node auf einen späteren
  Umlauf. Das zeigen das Etikett „+N Umläufe“ am gewählten Node und die Liste.
  Auf einer stabilen Bahn liegen die Umläufe übereinander, dort bleibt der
  Node im ersten (später: „+1 Umlauf“ im Node-Manager).
- **Scharf schalten pro Kette** (`Trajectory::armed`): Nur eine scharfe Kette
  wird ausgeführt. In der Simulation ist eine neue Kette nicht scharf. Eine
  nicht scharfe rückt bei Brennbeginn als Ganzes einen Umlauf weiter, so geht
  kein Plan verloren, nur weil der Zeitraffer läuft. Die Vorschau ist scharf
  wie nicht scharf dieselbe. Der Zeitraffer-Stopp greift nur bei scharfen
  Ketten. Eine Kette passt auch zu späteren Routen. **Vorerst schaltet das
  Spiel jede neue Kette sofort scharf**; die Schaltfläche dafür kommt später
  an anderer Stelle in die Oberfläche (siehe UI-Ideen).
- **Ungültige Nodes bleiben erhalten** (siehe Physikmodell), damit beim
  Ausprobieren am frühen Node kein späterer Plan verloren geht.
- **Brennpunkte:** Gelbe Ringe auf der Bahn markieren Beginn und Ende jedes
  geplanten Brennens, für alle Schiffe und alle gültigen Nodes.
- **Griffe:** Ein gezogener Griff folgt der Maus entlang seiner Achse und
  springt beim Loslassen zurück (wie in KSP). Er kann das Δv nicht über den
  Tankinhalt am Brennbeginn hinaus erhöhen. Das begrenzt nur die Bedienung;
  die Simulation erlaubt mehr Δv als im Tank ist (das Brennen endet dann
  leer), damit später vor einem Node nachgetankt werden kann.
- **Während des Brennens** zeigt die Karte die momentane Bahn (die Keplerbahn
  aus dem aktuellen Zustand, wie in KSP). Reine Darstellung, die Simulation
  rechnet unverändert den ganzen Brennvorgang voraus.
- **Zeitraffer-Stopp pro Schiff** (`halt_warp_at_burn`, Taste `H`, anfangs aus):
  Beginnt ein markiertes Schiff zu brennen, hält der Zeitraffer genau am
  Brennbeginn an und schaltet auf 1×. Pro Schiff statt für alle, damit spätere
  automatische Routen den Zeitraffer nicht ständig bremsen. Die Markierung
  gehört zum Spielstand.
- **Startprofil** mit wenigen Parametern (Kurs, Wendebeginn, Ziel-Apoapsis);
  das Schiff kommt mit nicht mehr vollen Tanks oben an. Zirkularisieren per
  Manöver-Node, auch schon vor dem Abheben geplant. Feste Startzeit mit
  Countdown (Vorgabe 2 min); verschieben verschiebt die ganze Kette.
- **Seitenleiste rechts** für alles, was man bedient, als Karten je nach Lage
  (zuerst „Start“ bei einem gelandeten Schiff). Die Anzeige links bleibt reine
  Information. Klicks auf die Leiste gehen nicht an die Karte dahinter.
  Schaltflächen prüfen Treffer selbst, ohne Bevys Picking.
- **Δv-Balken unten** für das gewählte Schiff: volle Breite = voller Tank
  (`VesselParams::fuel_capacity`), gefüllt = Δv an Bord. Darin die geplanten
  Brennvorgänge der Kette in Reihenfolge (Start gelb, Nodes abwechselnd
  violett, der gewählte hell umrandet), dann der Rest (grün). Verlangt die
  Kette mehr als an Bord ist, steht der Fehlbetrag rot dahinter (der Balken
  wird dafür länger). Teilstriche alle 500 m/s. Die Teile gehen exakt auf, weil
  sich bei fester Ausströmgeschwindigkeit das Δv der Brennvorgänge zum Δv des
  Tanks addiert (`World::delta_v_budget`).
- **Landung über den Manöver-Node:** Die Vorschau rechnet bis zum Boden,
  Aufschlagpunkt als Kreuz, Zielgebiet als Kreisfläche auf der Oberfläche.
  Fallschirme = größere Widerstandsfläche ab einer Auslösehöhe (dasselbe Modell).
  Letzte Bremsung abstrahiert: Restgeschwindigkeit = nötiges Delta-v.
- **Ziel verfehlt:** Mit Treibstoff ein „Hop" (= Startprofil mit niedriger
  Apoapsis, keine eigene Mechanik), ohne Treibstoff oder bei Absturz ein Wrack.
- **Satelliten** sind normale Schiffe. Zu tief ausgesetzt = Fehlschlag (Absturz).
- **Rendezvous:** nächste Annäherung anzeigen, mit Manöver-Nodes verbessern,
  Andocken abstrahiert (kostet Relativgeschwindigkeit als Delta-v),
  Abschnittstyp „angedockt an X".
- **Basis-Schiff:** Rohbau im Orbit, Transporter liefern „Bauteile" per
  Andocken, voll → Vorlage wechselt zum Basis-Schiff. Landet einmalig und
  gründet die Basis dort, wo es aufsetzt.
- **Booster:** abstrahierte Dienstleistung eines Raumhafens (Gebäude einer Basis):
  Kapazität, Rohstoffkosten, Abklingzeit. Das Schiff erscheint mit vollen Tanks
  in einer Parkbahn (echte Keplerbahn aus Breite und Richtung). Boost-back nur
  als Animation.
- **Vessel-Klassen und Gebäude** sind Vorlagen mit Werten; Upgrades ändern Werte.
  Fähigkeiten (Fracht, Antenne, Scanner) sind optionale Felder, keine eigene
  Logik pro Klasse.
- **Ein Auftragssystem** für Geld und Forschungspunkte.

## Merkliste (nicht vergessen)

- Kommunikationsnetz (Verbindung, Reichweite)
- Scans, automatisierte Routen, Ressourcen, Basen, Wirtschaft
- Bergung von Wracks (über Geld/Punkte)
- „Hop zum Ziel": Parameter automatisch suchen, falls Zielen per Hand zu fummelig ist
- **Geparkt:** Bandbreite, Modulslots, Gelände/Höhenkarte, Wiedereintrittshitze
- **Gestrichen:** Technikalterung, Lagrange-Punkte (mit Patched Conics nicht möglich)

## UI-Ideen (nach dem Grundgerüst)

Erst kommt das Grundgerüst, dann die Bedienung. Ideen landen hier, damit
nichts verloren geht.

- **Manöver-Node-Manager:** Übersicht und Bearbeitung der geplanten Nodes
  (sinnvoll, sobald es Manöverketten gibt)
- **Scharf schalten:** Schaltfläche dafür, z. B. im Node-Manager. Die Mechanik
  steckt schon in der Simulation; bis dahin ist jede Kette sofort scharf.
- **„+1 Umlauf“ / „−1 Umlauf“** für den gewählten Node, z. B. im Node-Manager.
  Damit kommt ein Node auch auf einer stabilen Bahn auf einen späteren Umlauf.
- **Liste der Nodes anklickbar** statt `↑`/`↓`.
- Alles davon als Karten in der **Seitenleiste** (gibt es seit dem Startprofil).
- **Startfenster:** Startzeit so wählen, dass die Bahnebene zu einem Ziel passt.

## Reihenfolge

1. Fundament ✓
2. Manöver-Node in 3D ✓ (Schub, endliche Brenndauer, Aufschlagkreuz; dazu
   momentane Bahn beim Brennen, Zeitraffer-Stopp, scharf schalten,
   nächster Umlauf, Griffe mit Tank-Anschlag)
3. Manöverketten ✓ (mehrere Nodes pro Schiff, Nodes auf zukünftigen Bahnen,
   ungültige Nodes bleiben, Anzeige „+N Umläufe“)
4. Startprofil ✓ (Kurs, Wendebeginn, Ziel-Apoapsis; Zirkularisieren per Node
   in derselben Kette; Seitenleiste mit Karte „Start“)
5. Landung (Zielgebiete, Fallschirme, Aufsetzen, Hop, Wrack)
6. Rendezvous und Andocken
7. Mond, Wechsel der Einflusssphäre
8. Fracht und Rohstoffe → Raumhafen und Booster → Rohbau und Basis-Schiff → Basen und Wirtschaft
